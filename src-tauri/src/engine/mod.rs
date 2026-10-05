//! Deterministic discovery engine:
//! processes + sockets → tree → project roots → grouping → stack →
//! components → port ownership → HTTP evidence.
//!
//! The engine is pure with respect to React and SQLite. It performs only
//! bounded filesystem reads (cached) and loopback HTTP probes.

pub mod classifier;
pub mod grouping;
pub mod http_probe;
pub mod ports;
pub mod process_tree;
pub mod project_detector;
pub mod providers;
pub mod raw;
pub mod stack_detector;
pub mod start_options;
pub mod tooling;
pub mod url;

use crate::models::{
    AccessLevel, AccessSummary, Component, ComponentStatus, Metrics, ProcessInfo, ProcessOrigin,
};
use grouping::group_processes;
use http_probe::{HttpProber, ThreadedProber};
use ports::{resolve_ports, DiscoveredPort, PortContext};
use process_tree::ProcessTree;
use project_detector::{DetectorConfig, ProjectDetector, ProjectRoot};
use raw::{RawProcess, SocketScan};
use stack_detector::{StackCache, StackFingerprint};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tooling::redact_args;

/// How often a running project's marker files are re-checked for changes.
pub const RUNNING_STACK_TTL: Duration = Duration::from_secs(30);
/// Stopped (remembered) projects are re-checked far less often.
pub const STOPPED_STACK_TTL: Duration = Duration::from_secs(300);
const PROJECT_ROOT_TTL: Duration = Duration::from_secs(30);

pub struct ScanInput<'a> {
    pub processes: &'a [RawProcess],
    /// `None` when socket discovery failed; projects/processes still resolve.
    pub sockets: Option<&'a SocketScan>,
    /// PIDs spawned by LocalDeck; their whole trees are `LocalDeckManaged`.
    pub managed_roots: &'a HashSet<u32>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub root: ProjectRoot,
    pub stack: Arc<StackFingerprint>,
    pub components: Vec<Component>,
    /// Tree-ordered, component after component.
    pub processes: Vec<ProcessInfo>,
    pub metrics: Metrics,
}

#[derive(Debug, Default)]
pub struct EngineOutput {
    /// Ascending by root path.
    pub projects: Vec<DiscoveredProject>,
    pub ports: Vec<DiscoveredPort>,
    pub access: AccessSummary,
}

pub struct Engine {
    detector: ProjectDetector,
    stacks: StackCache,
    prober: Box<dyn HttpProber>,
}

impl Engine {
    pub fn new(home: Option<PathBuf>) -> Self {
        Self::with_prober(home, Box::new(ThreadedProber::new()))
    }

    pub fn with_prober(home: Option<PathBuf>, prober: Box<dyn HttpProber>) -> Self {
        Self {
            detector: ProjectDetector::new(DetectorConfig::new(home), PROJECT_ROOT_TTL),
            stacks: StackCache::default(),
            prober,
        }
    }

    /// Cached stack fingerprint (also used for remembered, stopped projects).
    pub fn stack_for(&mut self, root: &Path, now: Instant, ttl: Duration) -> Arc<StackFingerprint> {
        self.stacks.get(root, now, ttl)
    }

    /// User-requested "refresh detection".
    pub fn invalidate(&mut self, root: &Path) {
        self.stacks.invalidate(root);
        self.detector.forget_root(root);
    }

    pub fn discover(&mut self, input: &ScanInput, now: Instant) -> EngineOutput {
        let processes = input.processes;
        let tree = ProcessTree::build(processes);

        let seen_cwds: HashSet<PathBuf> = processes.iter().filter_map(|p| p.cwd.clone()).collect();
        self.detector.retain_seen(&seen_cwds);

        let grouping = {
            let detector = &mut self.detector;
            group_processes(processes, &tree, &mut |cwd| detector.detect(cwd, now))
        };

        let by_pid: HashMap<u32, &RawProcess> = processes.iter().map(|p| (p.pid, p)).collect();
        let managed = managed_pids(&tree, input.managed_roots);

        let mut stacks: HashMap<PathBuf, Arc<StackFingerprint>> = HashMap::new();
        let mut component_ids: HashMap<(PathBuf, usize), String> = HashMap::new();
        let mut projects = Vec::new();

        for group in &grouping.projects {
            let stack = self.stacks.get(&group.root.path, now, RUNNING_STACK_TTL);
            stacks.insert(group.root.path.clone(), stack.clone());

            let mut components = Vec::new();
            let mut infos = Vec::new();
            let mut total = Metrics::default();
            for (index, comp) in group.components.iter().enumerate() {
                let members: Vec<&RawProcess> = comp
                    .pids
                    .iter()
                    .filter_map(|pid| by_pid.get(pid).copied())
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let classification = classifier::classify(&members, &group.root.path, &stack);
                let root_process = members[0];
                let id = format!(
                    "c{}-{}",
                    comp.root_pid,
                    root_process.started_at.unwrap_or(0)
                );
                component_ids.insert((group.root.path.clone(), index), id.clone());

                let mut depth_of: HashMap<u32, u32> = HashMap::new();
                let mut metrics = Metrics::default();
                let mut limited = false;
                for process in &members {
                    let depth = tree
                        .parent_of(process.pid)
                        .and_then(|parent| depth_of.get(&parent))
                        .map(|d| d + 1)
                        .unwrap_or(0);
                    depth_of.insert(process.pid, depth);
                    metrics.cpu_percent += process.cpu_usage;
                    metrics.memory_bytes += process.memory_bytes;
                    metrics.process_count += 1;
                    limited |= process.access != AccessLevel::Full;
                    infos.push(process_info(
                        process,
                        depth,
                        &id,
                        if managed.contains(&process.pid) {
                            ProcessOrigin::LocalDeckManaged
                        } else {
                            ProcessOrigin::External
                        },
                    ));
                }
                total.cpu_percent += metrics.cpu_percent;
                total.memory_bytes += metrics.memory_bytes;
                total.process_count += metrics.process_count;
                components.push(Component {
                    id,
                    key: classification.key,
                    name: classification.name,
                    kind: classification.kind,
                    detected_kind: classification.kind,
                    kind_overridden: false,
                    technology: classification.technology,
                    root_pid: comp.root_pid,
                    process_ids: comp.pids.clone(),
                    ports: Vec::new(),
                    status: if limited {
                        ComponentStatus::Limited
                    } else {
                        ComponentStatus::Running
                    },
                    origin: if managed.contains(&comp.root_pid) {
                        ProcessOrigin::LocalDeckManaged
                    } else {
                        ProcessOrigin::External
                    },
                    command: classification.command,
                    metrics,
                });
            }
            projects.push(DiscoveredProject {
                root: group.root.clone(),
                stack,
                components,
                processes: infos,
                metrics: total,
            });
        }

        let mut ports = match input.sockets {
            Some(scan) => resolve_ports(
                scan,
                &PortContext {
                    processes: &by_pid,
                    grouping: &grouping,
                    stacks: &stacks,
                    component_ids: &component_ids,
                },
                self.prober.as_mut(),
                now,
            ),
            None => Vec::new(),
        };
        attach_ports_to_components(&mut projects, &grouping, &mut ports);

        EngineOutput {
            projects,
            ports,
            access: access_summary(processes),
        }
    }
}

fn managed_pids(tree: &ProcessTree, roots: &HashSet<u32>) -> HashSet<u32> {
    let mut managed = HashSet::new();
    for root in roots {
        if !tree.contains(*root) {
            continue;
        }
        managed.insert(*root);
        managed.extend(tree.descendants(*root));
    }
    managed
}

fn process_info(
    process: &RawProcess,
    depth: u32,
    component_id: &str,
    origin: ProcessOrigin,
) -> ProcessInfo {
    ProcessInfo {
        pid: process.pid,
        parent_pid: process.parent_pid,
        name: process.name.clone(),
        executable: process
            .executable
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        args: redact_args(&process.args),
        cwd: process
            .cwd
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        cpu_usage: process.cpu_usage,
        memory_bytes: process.memory_bytes,
        started_at: process.started_at,
        access_level: process.access,
        origin,
        component_id: Some(component_id.to_string()),
        depth,
    }
}

fn access_summary(processes: &[RawProcess]) -> AccessSummary {
    let mut summary = AccessSummary {
        total: processes.len() as u32,
        ..Default::default()
    };
    for process in processes {
        match process.access {
            AccessLevel::Full => summary.full += 1,
            AccessLevel::Partial => summary.partial += 1,
            AccessLevel::Unavailable => summary.unavailable += 1,
        }
    }
    summary
}

fn attach_ports_to_components(
    projects: &mut [DiscoveredProject],
    grouping: &grouping::Grouping,
    ports: &mut [DiscoveredPort],
) {
    for port in ports.iter() {
        // Every component that holds one of the endpoint's PIDs lists the port.
        for process in &port.info.processes {
            let Some(owner) = grouping.owner_of.get(&process.pid) else {
                continue;
            };
            let Some(project) = projects.iter_mut().find(|p| p.root.path == owner.root) else {
                continue;
            };
            let Some(group) = grouping
                .projects
                .iter()
                .find(|g| g.root.path == owner.root)
                .and_then(|g| g.components.get(owner.component))
            else {
                continue;
            };
            if let Some(component) = project
                .components
                .iter_mut()
                .find(|c| c.root_pid == group.root_pid)
            {
                if !component.ports.contains(&port.info.port) {
                    component.ports.push(port.info.port);
                    component.ports.sort_unstable();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::process_tree::test_support::proc;
    use super::raw::RawSocket;
    use super::*;
    use crate::models::{ComponentKind, HttpState, PortOwnership, RootKind};
    use std::fs;
    use tempfile::TempDir;

    struct StaticProber(http_probe::ProbeResult);
    impl HttpProber for StaticProber {
        fn probe(
            &mut self,
            _k: &http_probe::ProbeKey,
            _t: std::net::SocketAddr,
            _n: Instant,
        ) -> http_probe::ProbeResult {
            self.0
        }
        fn retain(&mut self, _l: &HashSet<http_probe::ProbeKey>) {}
    }

    fn engine() -> Engine {
        Engine::with_prober(
            None,
            Box::new(StaticProber(http_probe::ProbeResult::NotHttp)),
        )
    }

    fn make_tauri_project(tmp: &TempDir) -> PathBuf {
        let root = tmp.path().join("Projeler/Yerel Güverte");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("src-tauri")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"localdeck","scripts":{"dev":"vite"},"devDependencies":{"vite":"^8","@tauri-apps/cli":"^2"}}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "").unwrap();
        fs::write(
            root.join("src-tauri/Cargo.toml"),
            "[package]\nname=\"localdeck\"\n",
        )
        .unwrap();
        fs::canonicalize(&root).unwrap()
    }

    fn sockets(list: Vec<RawSocket>) -> SocketScan {
        SocketScan {
            sockets: list,
            skipped: 0,
        }
    }

    #[test]
    fn end_to_end_real_fixture_project_with_ports_and_unassigned_postgres() {
        let tmp = TempDir::new().unwrap();
        let root = make_tauri_project(&tmp);
        let r = root.to_string_lossy().to_string();
        let backend = root.join("src-tauri").to_string_lossy().to_string();
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(5, Some(1), "zsh", &["-zsh"], Some(&r)),
            proc(10, Some(5), "bun", &["bun", "run", "dev"], Some(&r)),
            proc(11, Some(10), "sh", &["sh", "-c", "vite"], Some(&r)),
            proc(
                12,
                Some(11),
                "node",
                &["node", "node_modules/.bin/vite"],
                Some(&r),
            ),
            proc(
                13,
                Some(12),
                "esbuild",
                &["esbuild", "--service=0.21"],
                Some(&r),
            ),
            proc(20, Some(1), "cargo", &["cargo", "run"], Some(&backend)),
            proc(
                90,
                Some(1),
                "postgres",
                &["postgres", "-D", "/opt/pg"],
                Some("/opt/pg"),
            ),
        ];
        let scan = sockets(vec![
            RawSocket {
                address: "::1".parse().unwrap(),
                port: 1420,
                pids: vec![12],
            },
            RawSocket {
                address: "127.0.0.1".parse().unwrap(),
                port: 5432,
                pids: vec![90],
            },
        ]);
        let managed = HashSet::new();
        let mut engine = engine();
        let out = engine.discover(
            &ScanInput {
                processes: &procs,
                sockets: Some(&scan),
                managed_roots: &managed,
            },
            Instant::now(),
        );

        assert_eq!(
            out.projects.len(),
            1,
            "frontend + src-tauri + shell resolve to one project"
        );
        let project = &out.projects[0];
        assert_eq!(project.root.path, root);
        assert_eq!(project.root.kind, RootKind::Git);
        assert_eq!(project.stack.detected_name, "Localdeck");
        assert!(
            project.stack.has("tauri") && project.stack.has("vite") && project.stack.has("bun")
        );

        assert_eq!(project.components.len(), 2);
        let web = project
            .components
            .iter()
            .find(|c| c.root_pid == 10)
            .unwrap();
        assert_eq!(web.kind, ComponentKind::Web);
        assert_eq!(web.technology.as_deref(), Some("Vite"));
        assert_eq!(web.process_ids, vec![10, 11, 12, 13]);
        assert_eq!(web.ports, vec![1420]);
        assert_eq!(web.metrics.process_count, 4);
        assert!(
            !project.processes.iter().any(|p| p.pid == 5),
            "interactive shell excluded"
        );
        let esbuild = project.processes.iter().find(|p| p.pid == 13).unwrap();
        assert_eq!(esbuild.depth, 3);
        assert_eq!(project.metrics.process_count, 5);
        assert_eq!(project.metrics.memory_bytes, 5_000_000);

        let vite_port = out.ports.iter().find(|p| p.info.port == 1420).unwrap();
        assert_eq!(vite_port.info.ownership, PortOwnership::Assigned);
        assert_eq!(vite_port.info.http, HttpState::Likely);
        assert_eq!(vite_port.info.url.as_deref(), Some("http://localhost:1420"));
        assert_eq!(vite_port.owner_root.as_deref(), Some(root.as_path()));
        let pg = out.ports.iter().find(|p| p.info.port == 5432).unwrap();
        assert_eq!(pg.info.ownership, PortOwnership::Unassigned);
        assert_eq!(pg.owner_root, None);
        assert_eq!(out.access.total, 8);
    }

    #[test]
    fn managed_roots_mark_their_whole_tree_as_localdeck_managed() {
        let tmp = TempDir::new().unwrap();
        let root = make_tauri_project(&tmp);
        let r = root.to_string_lossy().to_string();
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(&r)),
            proc(
                11,
                Some(10),
                "node",
                &["node", "node_modules/.bin/vite"],
                Some(&r),
            ),
            proc(30, Some(1), "node", &["node", "other.js"], Some(&r)),
        ];
        let managed: HashSet<u32> = [10].into_iter().collect();
        let out = engine().discover(
            &ScanInput {
                processes: &procs,
                sockets: None,
                managed_roots: &managed,
            },
            Instant::now(),
        );
        let project = &out.projects[0];
        let origin = |pid: u32| {
            project
                .processes
                .iter()
                .find(|p| p.pid == pid)
                .unwrap()
                .origin
        };
        assert_eq!(origin(10), ProcessOrigin::LocalDeckManaged);
        assert_eq!(origin(11), ProcessOrigin::LocalDeckManaged);
        assert_eq!(origin(30), ProcessOrigin::External);
        let comp = |pid: u32| {
            project
                .components
                .iter()
                .find(|c| c.root_pid == pid)
                .unwrap()
        };
        assert_eq!(comp(10).origin, ProcessOrigin::LocalDeckManaged);
        assert_eq!(comp(30).origin, ProcessOrigin::External);
    }

    #[test]
    fn socket_scan_failure_still_yields_projects_without_ports() {
        let tmp = TempDir::new().unwrap();
        let root = make_tauri_project(&tmp);
        let r = root.to_string_lossy().to_string();
        let procs = vec![proc(
            10,
            None,
            "node",
            &["node", "node_modules/.bin/vite"],
            Some(&r),
        )];
        let out = engine().discover(
            &ScanInput {
                processes: &procs,
                sockets: None,
                managed_roots: &HashSet::new(),
            },
            Instant::now(),
        );
        assert_eq!(out.projects.len(), 1);
        assert!(out.ports.is_empty());
    }

    #[test]
    fn processes_with_missing_metadata_never_panic_and_are_counted() {
        let mut hidden = proc(7, Some(1), "launchd-child", &[], None);
        hidden.executable = None;
        hidden.access = AccessLevel::Unavailable;
        let mut partial = proc(8, Some(1), "thing", &["thing"], None);
        partial.access = AccessLevel::Partial;
        let procs = vec![proc(1, None, "launchd", &[], Some("/")), hidden, partial];
        let out = engine().discover(
            &ScanInput {
                processes: &procs,
                sockets: None,
                managed_roots: &HashSet::new(),
            },
            Instant::now(),
        );
        assert!(out.projects.is_empty());
        assert_eq!(out.access.total, 3);
        assert_eq!(out.access.unavailable, 1);
        assert_eq!(out.access.partial, 1);
        assert_eq!(out.access.full, 1);
    }

    #[test]
    fn limited_access_processes_mark_their_component_limited() {
        let tmp = TempDir::new().unwrap();
        let root = make_tauri_project(&tmp);
        let r = root.to_string_lossy().to_string();
        let mut partial = proc(10, None, "node", &["node", "server.js"], Some(&r));
        partial.access = AccessLevel::Partial;
        let out = engine().discover(
            &ScanInput {
                processes: &[partial],
                sockets: None,
                managed_roots: &HashSet::new(),
            },
            Instant::now(),
        );
        assert_eq!(
            out.projects[0].components[0].status,
            ComponentStatus::Limited
        );
    }

    #[test]
    fn process_arguments_are_redacted_before_leaving_the_engine() {
        let tmp = TempDir::new().unwrap();
        let root = make_tauri_project(&tmp);
        let r = root.to_string_lossy().to_string();
        let p = proc(
            10,
            None,
            "node",
            &["node", "server.js", "--token=hunter2"],
            Some(&r),
        );
        let out = engine().discover(
            &ScanInput {
                processes: &[p],
                sockets: None,
                managed_roots: &HashSet::new(),
            },
            Instant::now(),
        );
        let args = &out.projects[0].processes[0].args;
        assert_eq!(args[2], "--token=•••");
        assert!(!out.projects[0].components[0]
            .command
            .clone()
            .unwrap()
            .contains("hunter2"));
    }
}
