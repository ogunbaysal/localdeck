//! Pure composition of runtime facts + persisted records into the `Project`
//! model the UI renders (status, attention, primary URL/command, metrics).

use crate::engine::classifier::reduce_word;
use crate::engine::stack_detector::StackFingerprint;
use crate::engine::url::{select_primary_url, UrlCandidate};
use crate::engine::DiscoveredProject;
use crate::models::{
    ActivityEvent, AttentionItem, AttentionKind, ComponentKind, Metrics, PortInfo, PortOwnership,
    PrimaryCommand, Project, ProjectOverrides, RootKind, RuntimeStatus,
};
use crate::persistence::ProjectRecord;
use std::collections::{BTreeSet, HashMap};

pub struct ProjectFacts<'a> {
    pub record: &'a ProjectRecord,
    pub root_kind: RootKind,
    pub stack: &'a StackFingerprint,
    /// Present while the project has attributed running processes.
    pub discovered: Option<&'a DiscoveredProject>,
    /// This project's `Assigned`/`Shared` endpoints.
    pub ports: Vec<&'a PortInfo>,
    /// Ports whose ownership is ambiguous and involve this project.
    pub conflict_ports: BTreeSet<u16>,
    /// `(component key, name)` of components that vanished while the project
    /// kept running.
    pub lost_components: Vec<(String, String)>,
    pub stop_pending: bool,
    pub transient: Vec<AttentionItem>,
    pub recent_activity: Vec<ActivityEvent>,
    pub now_ms: i64,
}

/// Evidence tokens that mean "this process belongs to the primary command":
/// the normalised program+args, and the first words of the script body.
fn command_sequences(command: &PrimaryCommand, stack: &StackFingerprint) -> Vec<Vec<String>> {
    let mut sequences = Vec::new();
    let mut spawn = vec![reduce_word(&command.program)];
    spawn.extend(command.args.iter().map(|a| reduce_word(a)));
    sequences.push(spawn);

    let body = command
        .script
        .as_deref()
        .and_then(|name| stack.script(name))
        .map(|s| s.command.as_str());
    if let Some(body) = body {
        let mut words = Vec::new();
        for token in body.split_whitespace() {
            if matches!(token, "&&" | "||" | ";" | "|" | "&") {
                break;
            }
            if token.starts_with('-') && !words.is_empty() {
                break;
            }
            if token.contains('=') && words.is_empty() {
                continue;
            }
            if words.is_empty() && matches!(token, "cross-env" | "env" | "npx" | "bunx") {
                continue;
            }
            words.push(reduce_word(token));
            if words.len() == 2 {
                break;
            }
        }
        if !words.is_empty() {
            sequences.push(words);
        }
    }
    sequences
}

fn contains_sequence(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Does any process of the project look like the configured primary command?
pub fn primary_command_is_running(
    command: &PrimaryCommand,
    stack: &StackFingerprint,
    processes: &[crate::models::ProcessInfo],
) -> bool {
    let sequences = command_sequences(command, stack);
    processes.iter().any(|process| {
        let mut words = vec![reduce_word(&process.name)];
        words.extend(process.args.iter().map(|a| reduce_word(a)));
        sequences.iter().any(|seq| contains_sequence(&words, seq))
    })
}

pub fn compose_project(facts: &ProjectFacts) -> Project {
    let record = facts.record;
    let mut components = facts
        .discovered
        .map(|d| d.components.clone())
        .unwrap_or_default();
    for component in &mut components {
        if let Some(kind) = record.component_kinds.get(&component.key) {
            component.kind = *kind;
            component.kind_overridden = *kind != component.detected_kind;
        }
    }
    let processes = facts
        .discovered
        .map(|d| d.processes.clone())
        .unwrap_or_default();
    let metrics = facts.discovered.map(|d| d.metrics).unwrap_or_default();
    let active = !components.is_empty();

    let primary_missing = active
        && record
            .primary_command
            .as_ref()
            .is_some_and(|cmd| !primary_command_is_running(cmd, facts.stack, &processes));
    let runtime_status = if !active {
        RuntimeStatus::Stopped
    } else if primary_missing || !facts.lost_components.is_empty() {
        RuntimeStatus::Partial
    } else {
        RuntimeStatus::Running
    };

    // Primary URL: override → web component → known dev server → confirmed HTTP.
    let web_pids: BTreeSet<u32> = components
        .iter()
        .filter(|c| c.kind == ComponentKind::Web)
        .flat_map(|c| c.process_ids.iter().copied())
        .collect();
    let candidates: Vec<UrlCandidate> = facts
        .ports
        .iter()
        .filter_map(|port| {
            Some(UrlCandidate {
                port: port.port,
                url: port.url.clone()?,
                http: port.http,
                web_component: port.processes.iter().any(|p| web_pids.contains(&p.pid)),
            })
        })
        .collect();
    let primary_url = select_primary_url(record.primary_url.as_deref(), &candidates);

    let mut ports: Vec<u16> = facts.ports.iter().map(|p| p.port).collect();
    ports.sort_unstable();
    ports.dedup();

    let mut attention: Vec<AttentionItem> = Vec::new();
    for port in &facts.conflict_ports {
        attention.push(AttentionItem {
            id: format!("port-conflict-{port}"),
            kind: AttentionKind::PortConflict,
            message: format!(
                "Port {port} is held by processes that LocalDeck cannot attribute to a single project"
            ),
            dismissible: false,
        });
    }
    for (key, name) in &facts.lost_components {
        attention.push(AttentionItem {
            id: format!("runtime-lost-{key}"),
            kind: AttentionKind::RuntimeLost,
            message: format!("{name} stopped while the rest of the project kept running"),
            dismissible: false,
        });
    }
    attention.extend(facts.transient.iter().cloned());

    let detected_name = facts.stack.detected_name.clone();
    Project {
        id: record.id,
        name: record
            .display_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| detected_name.clone()),
        detected_name,
        root_path: record.root_path.clone(),
        root_kind: facts.root_kind,
        runtime_status,
        stop_pending: facts.stop_pending && active,
        attention,
        stack: facts.stack.stack.clone(),
        package_manager: facts.stack.package_manager,
        components,
        processes,
        ports,
        primary_url,
        primary_command: record.primary_command.clone(),
        overrides: ProjectOverrides {
            display_name: record.display_name.clone(),
            primary_url: record.primary_url.clone(),
            preferred_editor: record.preferred_editor.clone(),
        },
        metrics: if active { metrics } else { Metrics::default() },
        first_seen_at: record.first_seen_at,
        last_seen_at: if active {
            facts.now_ms
        } else {
            record.last_seen_at
        },
        favorite: record.favorite,
        ignored: record.ignored,
        recent_activity: facts.recent_activity.clone(),
    }
}

/// Ports owned by projects, indexed by project id.
pub fn ports_by_project(ports: &[PortInfo]) -> HashMap<i64, Vec<&PortInfo>> {
    let mut map: HashMap<i64, Vec<&PortInfo>> = HashMap::new();
    for port in ports {
        if matches!(
            port.ownership,
            PortOwnership::Assigned | PortOwnership::Shared
        ) {
            if let Some(id) = port.project_id {
                map.entry(id).or_default().push(port);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::project_detector::ProjectRoot;
    use crate::models::{
        AccessLevel, Component, ComponentStatus, HttpState, PackageManager, PackageScript,
        PortProcess, PrimaryUrlSource, ProcessInfo, ProcessOrigin, StackTag, Transport,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn record(id: i64) -> ProjectRecord {
        ProjectRecord {
            id,
            root_path: "/work/app".to_string(),
            display_name: None,
            first_seen_at: 1_000,
            last_seen_at: 2_000,
            favorite: false,
            ignored: false,
            primary_url: None,
            primary_command: None,
            preferred_editor: None,
            component_kinds: BTreeMap::new(),
        }
    }

    fn stack() -> StackFingerprint {
        StackFingerprint {
            detected_name: "App".to_string(),
            stack: vec![StackTag {
                id: "vite".into(),
                label: "Vite".into(),
            }],
            package_manager: Some(PackageManager::Bun),
            package_manager_detected: true,
            scripts: vec![PackageScript {
                name: "dev".into(),
                command: "vite --port 3000".into(),
            }],
        }
    }

    fn process(pid: u32, name: &str, args: &[&str], component: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: None,
            name: name.into(),
            executable: None,
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            cpu_usage: 1.5,
            memory_bytes: 10,
            started_at: Some(1),
            access_level: AccessLevel::Full,
            origin: ProcessOrigin::External,
            component_id: Some(component.into()),
            depth: 0,
        }
    }

    fn component(id: &str, key: &str, kind: ComponentKind, pids: &[u32]) -> Component {
        Component {
            id: id.into(),
            key: key.into(),
            name: key.into(),
            kind,
            detected_kind: kind,
            kind_overridden: false,
            technology: None,
            root_pid: pids[0],
            process_ids: pids.to_vec(),
            ports: vec![],
            status: ComponentStatus::Running,
            origin: ProcessOrigin::External,
            command: None,
            metrics: Metrics {
                cpu_percent: 1.5,
                memory_bytes: 10,
                process_count: pids.len() as u32,
            },
        }
    }

    fn discovered(components: Vec<Component>, processes: Vec<ProcessInfo>) -> DiscoveredProject {
        DiscoveredProject {
            root: ProjectRoot {
                path: "/work/app".into(),
                kind: RootKind::Git,
            },
            stack: Arc::new(stack()),
            metrics: Metrics {
                cpu_percent: 3.0,
                memory_bytes: 20,
                process_count: processes.len() as u32,
            },
            components,
            processes,
        }
    }

    fn port(port: u16, pid: u32, http: HttpState) -> PortInfo {
        let is_http = matches!(http, HttpState::Confirmed | HttpState::Likely);
        PortInfo {
            id: format!("{port}-{pid}"),
            port,
            addresses: vec!["127.0.0.1".into()],
            transport: Transport::Tcp,
            processes: vec![PortProcess {
                pid,
                name: None,
                started_at: None,
            }],
            owner_visible: true,
            ownership: PortOwnership::Assigned,
            project_id: Some(1),
            component_id: None,
            http,
            is_http,
            url: is_http.then(|| format!("http://localhost:{port}")),
        }
    }

    fn facts<'a>(
        record: &'a ProjectRecord,
        stack: &'a StackFingerprint,
        discovered: Option<&'a DiscoveredProject>,
        ports: Vec<&'a PortInfo>,
    ) -> ProjectFacts<'a> {
        ProjectFacts {
            record,
            root_kind: RootKind::Git,
            stack,
            discovered,
            ports,
            conflict_ports: BTreeSet::new(),
            lost_components: vec![],
            stop_pending: false,
            transient: vec![],
            recent_activity: vec![],
            now_ms: 9_000,
        }
    }

    #[test]
    fn stopped_project_has_no_runtime_data_and_keeps_remembered_times() {
        let record = record(1);
        let stack = stack();
        let project = compose_project(&facts(&record, &stack, None, vec![]));
        assert_eq!(project.runtime_status, RuntimeStatus::Stopped);
        assert_eq!(project.name, "App");
        assert!(project.components.is_empty() && project.processes.is_empty());
        assert_eq!(project.metrics, Metrics::default());
        assert_eq!(project.last_seen_at, 2_000);
        assert_eq!(project.primary_url, None);
        assert!(!project.stop_pending);
    }

    #[test]
    fn running_project_aggregates_backend_metrics_and_picks_the_web_url() {
        let record = record(1);
        let stack = stack();
        let d = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10, 11])],
            vec![
                process(10, "bun", &["bun", "run", "dev"], "c1"),
                process(11, "node", &["node", "vite"], "c1"),
            ],
        );
        let p = port(5173, 11, HttpState::Likely);
        let project = compose_project(&facts(&record, &stack, Some(&d), vec![&p]));
        assert_eq!(project.runtime_status, RuntimeStatus::Running);
        assert_eq!(project.metrics.process_count, 2);
        assert_eq!(project.ports, vec![5173]);
        let url = project.primary_url.unwrap();
        assert_eq!(url.url, "http://localhost:5173");
        assert_eq!(url.source, PrimaryUrlSource::WebComponent);
        assert_eq!(project.last_seen_at, 9_000);
    }

    #[test]
    fn user_override_url_and_name_win() {
        let mut record = record(1);
        record.display_name = Some("My Deck".into());
        record.primary_url = Some("https://deck.test".into());
        let stack = stack();
        let d = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10])],
            vec![process(10, "node", &["node", "vite"], "c1")],
        );
        let p = port(5173, 10, HttpState::Likely);
        let project = compose_project(&facts(&record, &stack, Some(&d), vec![&p]));
        assert_eq!(project.name, "My Deck");
        assert_eq!(project.detected_name, "App");
        assert_eq!(
            project.primary_url.unwrap().source,
            PrimaryUrlSource::Override
        );
        assert_eq!(project.overrides.display_name.as_deref(), Some("My Deck"));
    }

    #[test]
    fn component_kind_override_changes_url_selection_and_is_flagged() {
        let mut record = record(1);
        record
            .component_kinds
            .insert("srv@.".into(), ComponentKind::Web);
        let stack = stack();
        let d = discovered(
            vec![
                component("c1", "srv@.", ComponentKind::Runtime, &[10]),
                component("c2", "api@.", ComponentKind::Runtime, &[20]),
            ],
            vec![
                process(10, "node", &["node", "a.js"], "c1"),
                process(20, "node", &["node", "b.js"], "c2"),
            ],
        );
        let confirmed_low = port(4000, 20, HttpState::Confirmed);
        let confirmed_web = port(9000, 10, HttpState::Confirmed);
        let project = compose_project(&facts(
            &record,
            &stack,
            Some(&d),
            vec![&confirmed_low, &confirmed_web],
        ));
        let web = project
            .components
            .iter()
            .find(|c| c.key == "srv@.")
            .unwrap();
        assert!(web.kind_overridden);
        assert_eq!(web.kind, ComponentKind::Web);
        assert_eq!(web.detected_kind, ComponentKind::Runtime);
        let url = project.primary_url.unwrap();
        assert_eq!(url.url, "http://localhost:9000");
        assert_eq!(url.source, PrimaryUrlSource::WebComponent);
    }

    #[test]
    fn override_equal_to_detected_kind_is_not_flagged() {
        let mut record = record(1);
        record
            .component_kinds
            .insert("vite@.".into(), ComponentKind::Web);
        let stack = stack();
        let d = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10])],
            vec![process(10, "node", &["node", "vite"], "c1")],
        );
        let project = compose_project(&facts(&record, &stack, Some(&d), vec![]));
        assert!(!project.components[0].kind_overridden);
    }

    #[test]
    fn partial_when_the_configured_primary_command_is_not_running() {
        let mut record = record(1);
        record.primary_command = Some(PrimaryCommand {
            program: "bun".into(),
            args: vec!["run".into(), "dev".into()],
            cwd: "/work/app".into(),
            label: "bun run dev".into(),
            script: Some("dev".into()),
        });
        let stack = stack();
        let worker_only = discovered(
            vec![component("c1", "worker@.", ComponentKind::Worker, &[10])],
            vec![process(10, "node", &["node", "worker.js"], "c1")],
        );
        let project = compose_project(&facts(&record, &stack, Some(&worker_only), vec![]));
        assert_eq!(project.runtime_status, RuntimeStatus::Partial);

        let running = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10, 11])],
            vec![
                process(10, "bun", &["bun", "run", "dev"], "c1"),
                process(11, "node", &["node", "vite"], "c1"),
            ],
        );
        assert_eq!(
            compose_project(&facts(&record, &stack, Some(&running), vec![])).runtime_status,
            RuntimeStatus::Running
        );

        // The script body (`vite`) is evidence too: started directly, not through bun.
        let direct = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10])],
            vec![process(
                10,
                "node",
                &["node", "/work/app/node_modules/.bin/vite", "--port", "3000"],
                "c1",
            )],
        );
        assert_eq!(
            compose_project(&facts(&record, &stack, Some(&direct), vec![])).runtime_status,
            RuntimeStatus::Running
        );
    }

    #[test]
    fn no_expectation_is_inferred_from_stack_metadata_alone() {
        let record = record(1); // no primary command, no history
        let stack = stack(); // Vite in the stack, but only a worker is running
        let d = discovered(
            vec![component("c1", "worker@.", ComponentKind::Worker, &[10])],
            vec![process(10, "node", &["node", "worker.js"], "c1")],
        );
        let project = compose_project(&facts(&record, &stack, Some(&d), vec![]));
        assert_eq!(project.runtime_status, RuntimeStatus::Running);
    }

    #[test]
    fn lost_components_make_it_partial_with_attention() {
        let record = record(1);
        let stack = stack();
        let d = discovered(
            vec![component("c1", "api@.", ComponentKind::Api, &[10])],
            vec![process(10, "node", &["node", "api.js"], "c1")],
        );
        let mut f = facts(&record, &stack, Some(&d), vec![]);
        f.lost_components = vec![("vite@.".into(), "Vite".into())];
        let project = compose_project(&f);
        assert_eq!(project.runtime_status, RuntimeStatus::Partial);
        assert_eq!(project.attention.len(), 1);
        assert_eq!(project.attention[0].kind, AttentionKind::RuntimeLost);
        assert!(!project.attention[0].dismissible);
    }

    #[test]
    fn stopped_project_can_carry_attention_orthogonally() {
        let record = record(1);
        let stack = stack();
        let mut f = facts(&record, &stack, None, vec![]);
        f.transient = vec![AttentionItem {
            id: "managed-exited".into(),
            kind: AttentionKind::ManagedExited,
            message: "exited".into(),
            dismissible: true,
        }];
        let project = compose_project(&f);
        assert_eq!(project.runtime_status, RuntimeStatus::Stopped);
        assert_eq!(project.attention.len(), 1);
    }

    #[test]
    fn port_conflicts_surface_as_attention() {
        let record = record(1);
        let stack = stack();
        let d = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10])],
            vec![process(10, "node", &["node", "vite"], "c1")],
        );
        let mut f = facts(&record, &stack, Some(&d), vec![]);
        f.conflict_ports = [3000u16].into_iter().collect();
        let project = compose_project(&f);
        assert_eq!(project.attention[0].kind, AttentionKind::PortConflict);
        assert!(project.attention[0].message.contains("3000"));
    }

    #[test]
    fn stop_pending_only_applies_while_active() {
        let record = record(1);
        let stack = stack();
        let mut f = facts(&record, &stack, None, vec![]);
        f.stop_pending = true;
        assert!(!compose_project(&f).stop_pending);
        let d = discovered(
            vec![component("c1", "vite@.", ComponentKind::Web, &[10])],
            vec![process(10, "node", &["node", "vite"], "c1")],
        );
        let mut f = facts(&record, &stack, Some(&d), vec![]);
        f.stop_pending = true;
        assert!(compose_project(&f).stop_pending);
    }

    #[test]
    fn ports_by_project_only_indexes_assigned_and_shared() {
        let mut ambiguous = port(1, 1, HttpState::None);
        ambiguous.ownership = PortOwnership::Ambiguous;
        ambiguous.project_id = None;
        let mut shared = port(2, 2, HttpState::None);
        shared.ownership = PortOwnership::Shared;
        let assigned = port(3, 3, HttpState::None);
        let all = [ambiguous, shared, assigned];
        let map = ports_by_project(&all);
        assert_eq!(map[&1].len(), 2);
        assert_eq!(map.len(), 1);
    }
}
