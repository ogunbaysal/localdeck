//! Port → PID → Component → Project resolution.
//!
//! Ownership is only claimed from evidence: every reported PID must resolve to
//! an attributed process. Anything else stays `Unassigned`; conflicting PIDs
//! stay `Ambiguous`. Nothing is ever guessed from a port number or a name.

use super::classifier::{debug_ports, is_non_http_service, serves_http_by_design};
use super::grouping::{Grouping, PidOwner};
use super::http_probe::{probe_target, HttpProber, ProbeKey, ProbeResult};
use super::raw::{RawProcess, RawSocket, SocketScan};
use super::stack_detector::StackFingerprint;
use super::url::http_url;
use crate::models::{HttpState, PortInfo, PortOwnership, PortProcess, Transport};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub struct PortContext<'a> {
    pub processes: &'a HashMap<u32, &'a RawProcess>,
    pub grouping: &'a Grouping,
    pub stacks: &'a HashMap<PathBuf, Arc<StackFingerprint>>,
    pub component_ids: &'a HashMap<(PathBuf, usize), String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredPort {
    pub info: PortInfo,
    /// Project root when ownership is `Assigned`/`Shared`.
    pub owner_root: Option<PathBuf>,
    pub owner_component: Option<usize>,
    /// Roots of every project that owns at least one reported PID (for
    /// `Ambiguous`, so each can be told about the conflict).
    pub candidate_roots: BTreeSet<PathBuf>,
}

fn ownership_of(
    pids: &[u32],
    grouping: &Grouping,
) -> (PortOwnership, Option<PidOwner>, BTreeSet<PathBuf>) {
    let owners: Vec<Option<&PidOwner>> = pids.iter().map(|p| grouping.owner_of.get(p)).collect();
    let candidate_roots: BTreeSet<PathBuf> = owners
        .iter()
        .flatten()
        .map(|owner| owner.root.clone())
        .collect();
    if pids.is_empty() || owners.iter().all(Option::is_none) {
        return (PortOwnership::Unassigned, None, candidate_roots);
    }
    if owners.iter().any(Option::is_none) || candidate_roots.len() > 1 {
        return (PortOwnership::Ambiguous, None, candidate_roots);
    }
    let first = owners[0].cloned();
    let same_component = owners
        .iter()
        .flatten()
        .all(|o| o.component == first.as_ref().map(|f| f.component).unwrap_or(0));
    if same_component {
        (PortOwnership::Assigned, first, candidate_roots)
    } else {
        (PortOwnership::Shared, first, candidate_roots)
    }
}

pub fn resolve_ports(
    scan: &SocketScan,
    ctx: &PortContext,
    prober: &mut dyn HttpProber,
    now: Instant,
) -> Vec<DiscoveredPort> {
    // Merge wildcard/loopback twins that the same PIDs hold.
    let mut merged: BTreeMap<(u16, Vec<u32>), Vec<IpAddr>> = BTreeMap::new();
    for RawSocket {
        address,
        port,
        pids,
    } in &scan.sockets
    {
        let mut pids = pids.clone();
        pids.sort_unstable();
        pids.dedup();
        let addresses = merged.entry((*port, pids)).or_default();
        if !addresses.contains(address) {
            addresses.push(*address);
        }
    }

    let default_stack = StackFingerprint::default();
    let mut live: HashSet<ProbeKey> = HashSet::new();
    let mut out = Vec::new();

    for ((port, pids), mut addresses) in merged {
        addresses.sort();
        let (ownership, owner, candidate_roots) = ownership_of(&pids, ctx.grouping);

        let mut known_dev = false;
        let mut known_non_http = false;
        for pid in &pids {
            let Some(process) = ctx.processes.get(pid) else {
                continue;
            };
            let (root, stack) = match ctx.grouping.owner_of.get(pid) {
                Some(owner) => (
                    owner.root.as_path(),
                    ctx.stacks
                        .get(&owner.root)
                        .map(Arc::as_ref)
                        .unwrap_or(&default_stack),
                ),
                None => (Path::new("/"), &default_stack),
            };
            known_dev |= serves_http_by_design(process, stack, root);
            known_non_http |=
                is_non_http_service(process, stack, root) || debug_ports(process).contains(&port);
        }

        let http = if known_non_http {
            HttpState::None
        } else if known_dev {
            HttpState::Likely
        } else if matches!(ownership, PortOwnership::Assigned | PortOwnership::Shared) {
            match probe_target(&addresses, port) {
                Some(target) => {
                    let key = ProbeKey {
                        port,
                        pids: pids.clone(),
                    };
                    live.insert(key.clone());
                    match prober.probe(&key, target, now) {
                        ProbeResult::Http => HttpState::Confirmed,
                        ProbeResult::Pending => HttpState::Probing,
                        ProbeResult::NotHttp => HttpState::None,
                    }
                }
                None => HttpState::None,
            }
        } else {
            HttpState::None
        };
        let is_http = matches!(http, HttpState::Confirmed | HttpState::Likely);

        let (owner_root, owner_component) = match &owner {
            Some(o) => (Some(o.root.clone()), Some(o.component)),
            None => (None, None),
        };
        let component_id = match (ownership, &owner) {
            (PortOwnership::Assigned, Some(o)) => ctx
                .component_ids
                .get(&(o.root.clone(), o.component))
                .cloned(),
            _ => None,
        };

        let id = if pids.is_empty() {
            format!("{port}-none")
        } else {
            format!(
                "{port}-{}",
                pids.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join("_")
            )
        };
        let info = PortInfo {
            id,
            port,
            addresses: addresses.iter().map(IpAddr::to_string).collect(),
            transport: Transport::Tcp,
            processes: pids
                .iter()
                .map(|pid| {
                    let process = ctx.processes.get(pid);
                    PortProcess {
                        pid: *pid,
                        name: process.map(|p| p.name.clone()),
                        started_at: process.and_then(|p| p.started_at),
                    }
                })
                .collect(),
            owner_visible: !pids.is_empty(),
            ownership,
            project_id: None,
            component_id,
            http,
            is_http,
            url: is_http.then(|| http_url(&addresses, port)),
        };
        out.push(DiscoveredPort {
            info,
            owner_root,
            owner_component,
            candidate_roots,
        });
    }
    prober.retain(&live);
    out
}

#[cfg(test)]
mod tests {
    use super::super::grouping::{ComponentGroup, ProjectGroup};
    use super::super::process_tree::test_support::proc;
    use super::super::project_detector::ProjectRoot;
    use super::*;
    use crate::models::RootKind;
    use std::net::SocketAddr;

    #[derive(Default)]
    struct FakeProber {
        answers: HashMap<u16, ProbeResult>,
        calls: Vec<u16>,
        retained: Option<HashSet<ProbeKey>>,
    }

    impl HttpProber for FakeProber {
        fn probe(&mut self, key: &ProbeKey, _t: SocketAddr, _n: Instant) -> ProbeResult {
            self.calls.push(key.port);
            self.answers
                .get(&key.port)
                .copied()
                .unwrap_or(ProbeResult::NotHttp)
        }
        fn retain(&mut self, live: &HashSet<ProbeKey>) {
            self.retained = Some(live.clone());
        }
    }

    fn sock(addr: &str, port: u16, pids: &[u32]) -> RawSocket {
        RawSocket {
            address: addr.parse().unwrap(),
            port,
            pids: pids.to_vec(),
        }
    }

    struct Fixture {
        procs: Vec<RawProcess>,
        grouping: Grouping,
    }

    fn root(path: &str) -> ProjectRoot {
        ProjectRoot {
            path: PathBuf::from(path),
            kind: RootKind::Git,
        }
    }

    /// Project A: pid 10 (vite web comp 0), pid 11 (api comp 1), pid 12 comp 1 child.
    /// Project B: pid 20 (comp 0). Unowned: pid 90 (postgres), 91 (node, cwd none).
    fn fixture() -> Fixture {
        let procs = vec![
            proc(
                10,
                Some(1),
                "node",
                &["node", "node_modules/.bin/vite"],
                Some("/work/a"),
            ),
            proc(11, Some(1), "node", &["node", "api.js"], Some("/work/a")),
            proc(
                12,
                Some(11),
                "node",
                &["node", "api.js", "--worker"],
                Some("/work/a"),
            ),
            proc(20, Some(1), "node", &["node", "server.js"], Some("/work/b")),
            proc(90, Some(1), "postgres", &["postgres"], Some("/opt/pg")),
            proc(91, Some(1), "node", &["node", "mystery.js"], None),
            proc(
                30,
                Some(1),
                "node",
                &["node", "--inspect", "dbg.js"],
                Some("/work/a"),
            ),
        ];
        let a = ProjectGroup {
            root: root("/work/a"),
            components: vec![
                ComponentGroup {
                    root_pid: 10,
                    pids: vec![10],
                },
                ComponentGroup {
                    root_pid: 11,
                    pids: vec![11, 12],
                },
                ComponentGroup {
                    root_pid: 30,
                    pids: vec![30],
                },
            ],
        };
        let b = ProjectGroup {
            root: root("/work/b"),
            components: vec![ComponentGroup {
                root_pid: 20,
                pids: vec![20],
            }],
        };
        let mut grouping = Grouping::default();
        for project in [&a, &b] {
            for (index, comp) in project.components.iter().enumerate() {
                for pid in &comp.pids {
                    grouping.owner_of.insert(
                        *pid,
                        PidOwner {
                            root: project.root.path.clone(),
                            component: index,
                        },
                    );
                }
            }
        }
        grouping.projects = vec![a, b];
        Fixture { procs, grouping }
    }

    fn resolve(
        f: &Fixture,
        sockets: Vec<RawSocket>,
        prober: &mut FakeProber,
    ) -> Vec<DiscoveredPort> {
        let by_pid: HashMap<u32, &RawProcess> = f.procs.iter().map(|p| (p.pid, p)).collect();
        let stacks = HashMap::new();
        let mut component_ids = HashMap::new();
        for project in &f.grouping.projects {
            for index in 0..project.components.len() {
                component_ids.insert(
                    (project.root.path.clone(), index),
                    format!("c-{}-{}", project.root.path.display(), index),
                );
            }
        }
        let ctx = PortContext {
            processes: &by_pid,
            grouping: &f.grouping,
            stacks: &stacks,
            component_ids: &component_ids,
        };
        resolve_ports(
            &SocketScan {
                sockets,
                skipped: 0,
            },
            &ctx,
            prober,
            Instant::now(),
        )
    }

    #[test]
    fn project_port_is_linked_through_pid_to_component_and_project() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("127.0.0.1", 5173, &[10])], &mut prober);
        let p = &ports[0];
        assert_eq!(p.info.ownership, PortOwnership::Assigned);
        assert_eq!(p.owner_root, Some(PathBuf::from("/work/a")));
        assert_eq!(p.owner_component, Some(0));
        assert_eq!(p.info.component_id.as_deref(), Some("c-/work/a-0"));
        assert_eq!(p.info.http, HttpState::Likely, "vite is a known dev server");
        assert_eq!(p.info.url.as_deref(), Some("http://localhost:5173"));
        assert!(prober.calls.is_empty(), "known dev servers are not probed");
    }

    #[test]
    fn global_postgres_stays_unassigned_and_is_never_probed() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("127.0.0.1", 5432, &[90])], &mut prober);
        let p = &ports[0];
        assert_eq!(p.info.ownership, PortOwnership::Unassigned);
        assert_eq!(p.owner_root, None);
        assert_eq!(p.info.project_id, None);
        assert_eq!(p.info.component_id, None);
        assert_eq!(p.info.http, HttpState::None);
        assert_eq!(p.info.url, None);
        assert_eq!(p.info.processes[0].name.as_deref(), Some("postgres"));
        assert!(prober.calls.is_empty());
    }

    #[test]
    fn unowned_process_without_cwd_evidence_is_unassigned_and_not_probed() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("0.0.0.0", 4000, &[91])], &mut prober);
        assert_eq!(ports[0].info.ownership, PortOwnership::Unassigned);
        assert!(prober.calls.is_empty(), "only project ports are probed");
    }

    #[test]
    fn mixed_owned_and_unowned_pids_stay_ambiguous() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("0.0.0.0", 8080, &[11, 91])], &mut prober);
        let p = &ports[0];
        assert_eq!(p.info.ownership, PortOwnership::Ambiguous);
        assert_eq!(p.owner_root, None);
        assert_eq!(p.info.processes.len(), 2, "all PIDs preserved");
        assert_eq!(p.candidate_roots.len(), 1);
        assert!(prober.calls.is_empty());
    }

    #[test]
    fn pids_from_different_projects_are_ambiguous_for_both() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("0.0.0.0", 3000, &[11, 20])], &mut prober);
        assert_eq!(ports[0].info.ownership, PortOwnership::Ambiguous);
        assert_eq!(ports[0].candidate_roots.len(), 2);
    }

    #[test]
    fn several_components_of_one_project_are_shared_not_ambiguous() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("0.0.0.0", 3000, &[10, 11])], &mut prober);
        let p = &ports[0];
        assert_eq!(p.info.ownership, PortOwnership::Shared);
        assert_eq!(p.owner_root, Some(PathBuf::from("/work/a")));
        assert_eq!(p.info.component_id, None);
    }

    #[test]
    fn cluster_workers_in_one_component_keep_every_pid_and_stay_assigned() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("0.0.0.0", 3000, &[12, 11])], &mut prober);
        let p = &ports[0];
        assert_eq!(p.info.ownership, PortOwnership::Assigned);
        assert_eq!(
            p.info.processes.iter().map(|x| x.pid).collect::<Vec<_>>(),
            vec![11, 12]
        );
    }

    #[test]
    fn sockets_without_visible_owner_are_unassigned_and_flagged() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("127.0.0.1", 7000, &[])], &mut prober);
        assert!(!ports[0].info.owner_visible);
        assert_eq!(ports[0].info.ownership, PortOwnership::Unassigned);
        assert_eq!(ports[0].info.id, "7000-none");
    }

    #[test]
    fn dead_pid_in_socket_table_does_not_panic_and_is_unassigned() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("127.0.0.1", 7001, &[424242])], &mut prober);
        assert_eq!(ports[0].info.ownership, PortOwnership::Unassigned);
        assert_eq!(ports[0].info.processes[0].name, None);
    }

    #[test]
    fn ipv4_and_ipv6_wildcards_held_by_the_same_pid_merge_into_one_row() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let sockets = vec![sock("::", 5173, &[10]), sock("0.0.0.0", 5173, &[10])];
        let ports = resolve(&f, sockets, &mut prober);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].info.addresses.len(), 2);
        assert_eq!(ports[0].info.url.as_deref(), Some("http://localhost:5173"));
    }

    #[test]
    fn different_owners_on_one_port_number_are_separate_rows() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let sockets = vec![sock("127.0.0.1", 5173, &[10]), sock("::1", 5173, &[20])];
        let ports = resolve(&f, sockets, &mut prober);
        assert_eq!(ports.len(), 2);
    }

    #[test]
    fn unknown_project_port_is_probed_and_reflects_the_probe_state() {
        let f = fixture();
        for (answer, expected, is_http) in [
            (ProbeResult::Http, HttpState::Confirmed, true),
            (ProbeResult::Pending, HttpState::Probing, false),
            (ProbeResult::NotHttp, HttpState::None, false),
        ] {
            let mut prober = FakeProber::default();
            prober.answers.insert(4000, answer);
            let ports = resolve(&f, vec![sock("127.0.0.1", 4000, &[20])], &mut prober);
            assert_eq!(prober.calls, vec![4000]);
            assert_eq!(ports[0].info.http, expected);
            assert_eq!(ports[0].info.is_http, is_http);
            assert_eq!(ports[0].info.url.is_some(), is_http);
        }
    }

    #[test]
    fn lan_only_listener_is_not_probed() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("192.168.1.9", 4000, &[20])], &mut prober);
        assert!(prober.calls.is_empty());
        assert_eq!(ports[0].info.http, HttpState::None);
    }

    #[test]
    fn debug_protocol_ports_are_not_probed() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let ports = resolve(&f, vec![sock("127.0.0.1", 9229, &[30])], &mut prober);
        assert!(prober.calls.is_empty());
        assert_eq!(ports[0].info.http, HttpState::None);
    }

    #[test]
    fn rows_are_sorted_by_port_and_probe_cache_is_pruned_to_live_endpoints() {
        let f = fixture();
        let mut prober = FakeProber::default();
        let sockets = vec![
            sock("127.0.0.1", 9000, &[20]),
            sock("127.0.0.1", 5432, &[90]),
            sock("127.0.0.1", 3000, &[20]),
        ];
        let ports = resolve(&f, sockets, &mut prober);
        let order: Vec<u16> = ports.iter().map(|p| p.info.port).collect();
        assert_eq!(order, vec![3000, 5432, 9000]);
        let retained = prober.retained.unwrap();
        assert_eq!(retained.len(), 2);
        assert!(retained.iter().all(|k| k.port == 3000 || k.port == 9000));
    }
}
