//! Rust-owned runtime state: persisted project registry, in-memory activity
//! mirror, managed-process registry and the latest `AppSnapshot`.
//!
//! One scan = [`Runtime::apply_scan`]. Lock order is always
//! `state` → `db` (engine is only held on its own).

use super::compose::{compose_project, ports_by_project, ProjectFacts};
use super::tracker::{Observation, Tracker};
use crate::engine::raw::{RawProcess, SocketScan};
use crate::engine::{Engine, ScanInput, STOPPED_STACK_TTL};
use crate::error::{AppError, AppResult, ErrorPayload};
use crate::models::{
    ActivityEvent, ActivityKind, ActivityMetadata, AppSnapshot, AttentionItem, AttentionKind,
    Platform, PortInfo, PortOwnership, PrimaryCommand, Project, ProjectId, RootKind, ScannerStatus,
    Settings,
};
use crate::persistence::{Database, NewActivity, ProjectRecord, ACTIVITY_LIMIT};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const SCAN_INTERVAL_MS: u32 = 2_000;
/// `last_seen_at` is persisted at most this often while a project is active.
const TOUCH_INTERVAL_MS: i64 = 60_000;
const PRUNE_INTERVAL_MS: i64 = 60 * 60 * 1000;
const RECENT_ACTIVITY_PER_PROJECT: usize = 15;
const ACTION_FAILURE_TTL_MS: i64 = 10 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic inside a scan must not wedge every later command.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Result of reading the OS for one scan.
pub struct ScanResult {
    pub processes: Vec<RawProcess>,
    pub sockets: Result<SocketScan, AppError>,
    pub duration_ms: u32,
}

pub(super) struct Entry {
    pub record: ProjectRecord,
    pub root_kind: RootKind,
}

pub(super) struct ManagedEntry {
    pub child: Child,
    pub pid: u32,
    pub command: PrimaryCommand,
    pub env: Vec<(String, String)>,
    pub profile_id: Option<String>,
    pub started: Instant,
    pub stop_requested: bool,
}

/// What LocalDeck last started for a project (repeated by restart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartRequest {
    pub profile_id: Option<String>,
    pub port: Option<u16>,
}

pub(super) struct PendingRestart {
    pub command: PrimaryCommand,
    pub env: Vec<(String, String)>,
    pub profile_id: Option<String>,
    pub due: Instant,
    pub attempt: u32,
}

#[derive(Default)]
pub(super) struct RestartBook {
    pub attempts: u32,
    pub pending: Vec<PendingRestart>,
}

/// Auto-restart pacing. Injectable so tests do not wait for real delays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    pub base_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
    pub max_attempts: u32,
    /// A run at least this long resets the attempt counter.
    pub reset_after: std::time::Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            base_delay: std::time::Duration::from_secs(2),
            max_delay: std::time::Duration::from_secs(30),
            max_attempts: 5,
            reset_after: std::time::Duration::from_secs(60),
        }
    }
}

impl RestartPolicy {
    /// `base · 2^(attempt-1)`, capped.
    pub fn delay(&self, attempt: u32) -> std::time::Duration {
        let factor = 1u32 << attempt.saturating_sub(1).min(16);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

/// A user-facing alert produced by a scan; delivered natively by the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserNotification {
    pub project_id: ProjectId,
    pub title: String,
    pub body: String,
}

const MAX_PENDING_NOTIFICATIONS: usize = 20;

pub(super) struct TransientItem {
    pub item: AttentionItem,
    pub created_ms: i64,
}

#[derive(Default)]
pub(super) struct State {
    pub registry: std::collections::BTreeMap<ProjectId, Entry>,
    pub by_path: HashMap<String, ProjectId>,
    pub tracker: Tracker,
    /// Oldest first, bounded.
    pub activity: VecDeque<ActivityEvent>,
    pub managed: HashMap<ProjectId, Vec<ManagedEntry>>,
    pub restarts: HashMap<ProjectId, RestartBook>,
    pub last_start: HashMap<ProjectId, StartRequest>,
    pub stop_requests: HashMap<ProjectId, Vec<(u32, Option<i64>)>>,
    pub transient: HashMap<ProjectId, Vec<TransientItem>>,
    pub last_touch: HashMap<ProjectId, i64>,
    pub scan_count: u64,
    pub persistence_error: Option<ErrorPayload>,
    /// `pid → started_at` for every process shown anywhere in the last snapshot.
    pub known_processes: HashMap<u32, Option<i64>>,
    pub live_pids: HashMap<u32, Option<i64>>,
    pub pending_events: Vec<NewActivity>,
    pub notifications: Vec<UserNotification>,
    pub last_prune_ms: i64,
}

impl State {
    pub(super) fn insert_record(&mut self, record: ProjectRecord, root_kind: RootKind) {
        self.by_path.insert(record.root_path.clone(), record.id);
        self.registry.insert(record.id, Entry { record, root_kind });
    }

    pub(super) fn remove_project(&mut self, id: ProjectId) {
        if let Some(entry) = self.registry.remove(&id) {
            self.by_path.remove(&entry.record.root_path);
        }
        self.stop_requests.remove(&id);
        self.transient.remove(&id);
        self.last_touch.remove(&id);
        self.activity.retain(|e| e.project_id != Some(id));
    }

    pub(super) fn push_transient(&mut self, project: ProjectId, item: AttentionItem, now_ms: i64) {
        let list = self.transient.entry(project).or_default();
        list.retain(|t| t.item.id != item.id);
        list.push(TransientItem {
            item,
            created_ms: now_ms,
        });
    }

    fn push_activity(&mut self, events: Vec<ActivityEvent>) {
        for event in events {
            self.activity.push_back(event);
        }
        while self.activity.len() > ACTIVITY_LIMIT {
            self.activity.pop_front();
        }
    }

    fn recent_for(&self, id: ProjectId) -> Vec<ActivityEvent> {
        self.activity
            .iter()
            .rev()
            .filter(|e| e.project_id == Some(id))
            .take(RECENT_ACTIVITY_PER_PROJECT)
            .cloned()
            .collect()
    }

    /// Roots of the processes LocalDeck itself spawned.
    pub(super) fn managed_pids(&self) -> HashSet<u32> {
        self.managed.values().flatten().map(|m| m.pid).collect()
    }

    /// Cancel scheduled auto-restarts and forget the attempt counter.
    pub(super) fn cancel_restarts(&mut self, id: ProjectId) {
        self.restarts.remove(&id);
    }

    /// Count a failed run and either schedule the next attempt or give up.
    pub(super) fn plan_restart(
        &mut self,
        project: ProjectId,
        entry: PendingRestart,
        ran_for: std::time::Duration,
        now: Instant,
        now_ms: i64,
        policy: &RestartPolicy,
    ) {
        let book = self.restarts.entry(project).or_default();
        if ran_for >= policy.reset_after {
            book.attempts = 0;
        }
        book.attempts += 1;
        if book.attempts > policy.max_attempts {
            book.pending.clear();
            self.pending_events.push(NewActivity {
                project_id: Some(project),
                kind: ActivityKind::AutoRestartGaveUp,
                metadata: ActivityMetadata {
                    attempt: Some(policy.max_attempts),
                    ..Default::default()
                },
                created_at: now_ms,
            });
            self.push_transient(
                project,
                AttentionItem {
                    id: "auto-restart-gave-up".to_string(),
                    kind: AttentionKind::AutoRestartGaveUp,
                    message: format!(
                        "`{}` kept crashing; auto-restart gave up after {} attempts",
                        entry.command.label, policy.max_attempts
                    ),
                    dismissible: true,
                },
                now_ms,
            );
            return;
        }
        book.pending.push(PendingRestart {
            due: now + policy.delay(book.attempts),
            attempt: book.attempts,
            ..entry
        });
    }

    /// Reap exited managed children; unexpected exits become activity +
    /// attention (and, with auto-restart on, a scheduled restart).
    fn reap_managed(&mut self, now_ms: i64, now: Instant, policy: &RestartPolicy) {
        let mut finished: Vec<(ProjectId, ManagedEntry, Option<i32>, bool)> = Vec::new();
        for (project, entries) in self.managed.iter_mut() {
            let mut index = 0;
            while index < entries.len() {
                match entries[index].child.try_wait() {
                    Ok(Some(status)) => {
                        let entry = entries.remove(index);
                        let expected = entry.stop_requested;
                        finished.push((*project, entry, status.code(), expected));
                    }
                    Ok(None) => index += 1,
                    Err(_) => {
                        let entry = entries.remove(index);
                        finished.push((*project, entry, None, true));
                    }
                }
            }
        }
        self.managed.retain(|_, entries| !entries.is_empty());
        for (project, entry, code, expected) in finished {
            if expected {
                continue;
            }
            self.pending_events.push(NewActivity {
                project_id: Some(project),
                kind: ActivityKind::ManagedExited,
                metadata: ActivityMetadata {
                    exit_code: code,
                    ..Default::default()
                },
                created_at: now_ms,
            });
            let how = match code {
                Some(code) => format!("exited with code {code}"),
                None => "was terminated".to_string(),
            };
            self.push_transient(
                project,
                AttentionItem {
                    id: "managed-exited".to_string(),
                    kind: AttentionKind::ManagedExited,
                    message: format!(
                        "`{}` {how} without being stopped from LocalDeck",
                        entry.command.label
                    ),
                    dismissible: true,
                },
                now_ms,
            );
            let auto = self
                .registry
                .get(&project)
                .is_some_and(|e| e.record.auto_restart);
            if auto {
                let ran_for = now.saturating_duration_since(entry.started);
                self.plan_restart(
                    project,
                    PendingRestart {
                        command: entry.command,
                        env: entry.env,
                        profile_id: entry.profile_id,
                        due: now,
                        attempt: 0,
                    },
                    ran_for,
                    now,
                    now_ms,
                    policy,
                );
            }
        }
    }

    fn stop_pending(&mut self, id: ProjectId) -> bool {
        let Some(requested) = self.stop_requests.get(&id) else {
            return false;
        };
        let alive = requested.iter().any(|(pid, started)| {
            self.live_pids
                .get(pid)
                .is_some_and(|live| started.is_none() || live == started)
        });
        if !alive {
            self.stop_requests.remove(&id);
        }
        alive
    }
}

pub struct Runtime {
    pub(super) db: Mutex<Database>,
    pub(super) engine: Mutex<Engine>,
    pub(super) state: Mutex<State>,
    snapshot: RwLock<Arc<AppSnapshot>>,
    pub(super) settings: Mutex<Settings>,
    pub(super) restart_policy: Mutex<RestartPolicy>,
    pub pacing: super::pacing::Pacing,
    scan_trigger: Mutex<Option<Sender<()>>>,
    /// Startup-time storage problem (for example the database could not be
    /// opened and LocalDeck fell back to in-memory state). Always surfaced.
    persistence_notice: Mutex<Option<ErrorPayload>>,
}

fn empty_snapshot(platform: Platform) -> AppSnapshot {
    AppSnapshot {
        projects: Vec::new(),
        ports: Vec::new(),
        scanner: ScannerStatus {
            interval_ms: SCAN_INTERVAL_MS,
            scan_count: 0,
            last_scan_ms: 0,
            process_access: Default::default(),
            sockets_available: true,
            socket_error: None,
            sockets_skipped: 0,
            persistence_error: None,
            platform,
        },
        updated_at: 0,
    }
}

impl Runtime {
    /// Loads remembered projects, activity and settings from the database.
    pub fn new(db: Database, engine: Engine) -> AppResult<Self> {
        let settings = db.load_settings()?;
        let mut state = State::default();
        for record in db.list_projects()? {
            let kind = crate::engine::project_detector::detect_root(
                Path::new(&record.root_path),
                &Default::default(),
            )
            .map(|r| r.kind)
            .unwrap_or(RootKind::Manifest);
            state.insert_record(record, kind);
        }
        let mut recent = db.recent_activity(ACTIVITY_LIMIT)?;
        recent.reverse();
        state.push_activity(recent);
        Ok(Self {
            db: Mutex::new(db),
            engine: Mutex::new(engine),
            state: Mutex::new(state),
            snapshot: RwLock::new(Arc::new(empty_snapshot(Platform::current()))),
            settings: Mutex::new(settings),
            restart_policy: Mutex::new(RestartPolicy::default()),
            pacing: super::pacing::Pacing::default(),
            scan_trigger: Mutex::new(None),
            persistence_notice: Mutex::new(None),
        })
    }

    /// Replace the auto-restart pacing (tests).
    pub fn set_restart_policy(&self, policy: RestartPolicy) {
        *lock(&self.restart_policy) = policy;
    }

    pub fn set_persistence_notice(&self, notice: ErrorPayload) {
        *lock(&self.persistence_notice) = Some(notice);
    }

    pub fn set_scan_trigger(&self, sender: Sender<()>) {
        *lock(&self.scan_trigger) = Some(sender);
    }

    /// Ask the scanner for an immediate scan (after an action or edit).
    pub fn request_scan(&self) {
        if let Some(sender) = lock(&self.scan_trigger).as_ref() {
            let _ = sender.send(());
        }
    }

    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Alerts produced since the last call (the shell shows them natively).
    pub fn drain_notifications(&self) -> Vec<UserNotification> {
        std::mem::take(&mut lock(&self.state).notifications)
    }

    /// Recent activity, newest first; optionally for a single project.
    pub fn list_activity(&self, project_id: Option<ProjectId>, limit: usize) -> Vec<ActivityEvent> {
        lock(&self.state)
            .activity
            .iter()
            .rev()
            .filter(|e| project_id.is_none() || e.project_id == project_id)
            .take(limit.min(ACTIVITY_LIMIT))
            .cloned()
            .collect()
    }

    pub fn clear_activity(&self) -> AppResult<()> {
        let mut st = lock(&self.state);
        lock(&self.db).clear_activity()?;
        st.activity.clear();
        drop(st);
        self.request_scan();
        Ok(())
    }

    pub fn find_project(&self, id: ProjectId) -> AppResult<Project> {
        self.snapshot()
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(AppError::ProjectNotFound { id })
    }

    pub fn apply_scan(&self, scan: ScanResult) -> Arc<AppSnapshot> {
        let now = Instant::now();
        let now_ms = unix_ms();

        // 1. Reap managed children; collect what the engine needs from state.
        let (managed_roots, known_paths) = {
            let mut st = lock(&self.state);
            let policy = *lock(&self.restart_policy);
            st.reap_managed(now_ms, now, &policy);
            st.live_pids = scan
                .processes
                .iter()
                .map(|p| (p.pid, p.started_at))
                .collect();
            let paths: Vec<PathBuf> = st
                .registry
                .values()
                .map(|e| PathBuf::from(&e.record.root_path))
                .collect();
            (st.managed_pids(), paths)
        };

        // 2. Discovery (own lock; includes stack fingerprints of stopped projects).
        let (socket_ref, socket_error) = match &scan.sockets {
            Ok(sockets) => (Some(sockets), None),
            Err(err) => (None, Some(err.payload())),
        };
        let (output, stopped_stacks) = {
            let mut engine = lock(&self.engine);
            let output = engine.discover(
                &ScanInput {
                    processes: &scan.processes,
                    sockets: socket_ref,
                    managed_roots: &managed_roots,
                },
                now,
            );
            let running: HashSet<&Path> = output
                .projects
                .iter()
                .map(|d| d.root.path.as_path())
                .collect();
            let mut stacks = HashMap::new();
            for path in &known_paths {
                if !running.contains(path.as_path()) {
                    stacks.insert(path.clone(), engine.stack_for(path, now, STOPPED_STACK_TTL));
                }
            }
            (output, stacks)
        };

        // 3. Merge with persisted identity and compose the snapshot.
        let mut st = lock(&self.state);
        let mut db = lock(&self.db);
        let mut persistence_error: Option<ErrorPayload> = None;

        let mut root_ids: HashMap<PathBuf, ProjectId> = HashMap::new();
        for d in &output.projects {
            let key = d.root.path.to_string_lossy().to_string();
            if let Some(id) = st.by_path.get(&key).copied() {
                if let Some(entry) = st.registry.get_mut(&id) {
                    entry.root_kind = d.root.kind;
                }
                root_ids.insert(d.root.path.clone(), id);
                continue;
            }
            match db.upsert_project(&key, now_ms) {
                Ok(record) => {
                    root_ids.insert(d.root.path.clone(), record.id);
                    st.insert_record(record, d.root.kind);
                }
                Err(err) => {
                    // A failed write must not look like successful discovery.
                    persistence_error = Some(err.payload());
                }
            }
        }

        let mut ports: Vec<PortInfo> = Vec::with_capacity(output.ports.len());
        let mut conflicts: HashMap<ProjectId, BTreeSet<u16>> = HashMap::new();
        for discovered in &output.ports {
            let mut info = discovered.info.clone();
            if let Some(root) = &discovered.owner_root {
                match root_ids.get(root) {
                    Some(id) => info.project_id = Some(*id),
                    None => {
                        info.ownership = PortOwnership::Unassigned;
                        info.component_id = None;
                    }
                }
            }
            if info.ownership == PortOwnership::Ambiguous {
                for root in &discovered.candidate_roots {
                    if let Some(id) = root_ids.get(root) {
                        conflicts.entry(*id).or_default().insert(info.port);
                    }
                }
            }
            ports.push(info);
        }
        let by_project = ports_by_project(&ports);

        let discovered_by_path: HashMap<String, &crate::engine::DiscoveredProject> = output
            .projects
            .iter()
            .map(|d| (d.root.path.to_string_lossy().to_string(), d))
            .collect();

        // Transitions → activity.
        let observations: Vec<Observation> = st
            .registry
            .values()
            .map(|entry| {
                let id = entry.record.id;
                let discovered = discovered_by_path.get(&entry.record.root_path);
                Observation {
                    id,
                    active: discovered.is_some_and(|d| !d.components.is_empty()),
                    ports: by_project
                        .get(&id)
                        .map(|list| list.iter().map(|p| p.port).collect())
                        .unwrap_or_default(),
                    components: discovered
                        .map(|d| {
                            d.components
                                .iter()
                                .map(|c| (c.key.clone(), c.name.clone(), !c.ports.is_empty()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    conflicts: conflicts.get(&id).cloned().unwrap_or_default(),
                }
            })
            .collect();
        let mut new_events = std::mem::take(&mut st.pending_events);
        new_events.extend(st.tracker.observe(&observations, now_ms));
        if !new_events.is_empty() {
            match db.insert_activity(&new_events) {
                Ok(stored) => st.push_activity(stored),
                Err(err) => persistence_error = Some(err.payload()),
            }
        }

        // Last-seen is throttled; a stop transition always records it.
        let stopped_now: HashSet<ProjectId> = new_events
            .iter()
            .filter(|e| e.kind == ActivityKind::ProjectStopped)
            .filter_map(|e| e.project_id)
            .collect();
        for obs in &observations {
            let due = st
                .last_touch
                .get(&obs.id)
                .is_none_or(|last| now_ms - last >= TOUCH_INTERVAL_MS);
            if (obs.active && due) || stopped_now.contains(&obs.id) {
                match db.touch_project(obs.id, now_ms) {
                    Ok(()) => {
                        st.last_touch.insert(obs.id, now_ms);
                        if let Some(entry) = st.registry.get_mut(&obs.id) {
                            entry.record.last_seen_at = now_ms;
                        }
                    }
                    Err(err) => persistence_error = Some(err.payload()),
                }
            }
            if obs.active {
                if let Some(items) = st.transient.get_mut(&obs.id) {
                    items.retain(|t| t.item.kind != AttentionKind::ManagedExited);
                }
            }
        }
        for items in st.transient.values_mut() {
            items.retain(|t| {
                now_ms - t.created_ms < ACTION_FAILURE_TTL_MS
                    || t.item.kind != AttentionKind::ActionFailed
            });
        }

        self.maybe_prune(&mut st, &mut db, now_ms);

        // Compose projects.
        let ids: Vec<ProjectId> = st.registry.keys().copied().collect();
        let mut projects = Vec::with_capacity(ids.len());
        for id in ids {
            let stop_pending = st.stop_pending(id);
            let Some(entry) = st.registry.get(&id) else {
                continue;
            };
            let discovered = discovered_by_path.get(&entry.record.root_path).copied();
            let stopped_stack;
            let stack = match discovered {
                Some(d) => d.stack.as_ref(),
                None => {
                    stopped_stack = stopped_stacks
                        .get(Path::new(&entry.record.root_path))
                        .cloned()
                        .unwrap_or_default();
                    stopped_stack.as_ref()
                }
            };
            let facts = ProjectFacts {
                record: &entry.record,
                root_kind: entry.root_kind,
                stack,
                discovered,
                ports: by_project.get(&id).cloned().unwrap_or_default(),
                conflict_ports: conflicts.get(&id).cloned().unwrap_or_default(),
                lost_components: st.tracker.lost_components(id),
                stop_pending,
                transient: st
                    .transient
                    .get(&id)
                    .map(|items| items.iter().map(|t| t.item.clone()).collect())
                    .unwrap_or_default(),
                recent_activity: st.recent_for(id),
                now_ms,
            };
            projects.push(compose_project(&facts));
        }

        let mut known: HashMap<u32, Option<i64>> = HashMap::new();
        for project in &projects {
            for process in &project.processes {
                known.insert(process.pid, process.started_at);
            }
        }
        for port in &ports {
            for process in &port.processes {
                known.entry(process.pid).or_insert(process.started_at);
            }
        }
        st.known_processes = known;

        st.scan_count += 1;
        if let Some(err) = persistence_error {
            st.persistence_error = Some(err);
        } else if !new_events.is_empty() || !root_ids.is_empty() {
            st.persistence_error = None;
        }
        if lock(&self.settings).notifications_enabled {
            for event in &new_events {
                let notify = matches!(
                    event.kind,
                    ActivityKind::ManagedExited
                        | ActivityKind::PortConflict
                        | ActivityKind::RuntimeLost
                        | ActivityKind::AutoRestartGaveUp
                );
                let Some(project_id) = event.project_id.filter(|_| notify) else {
                    continue;
                };
                let Some(project) = projects.iter().find(|p| p.id == project_id) else {
                    continue;
                };
                st.notifications.push(UserNotification {
                    project_id,
                    title: project.name.clone(),
                    body: ActivityEvent::describe(event.kind, &event.metadata),
                });
            }
            let excess = st
                .notifications
                .len()
                .saturating_sub(MAX_PENDING_NOTIFICATIONS);
            st.notifications.drain(..excess);
        }
        let snapshot = Arc::new(AppSnapshot {
            projects,
            ports,
            scanner: ScannerStatus {
                interval_ms: self.pacing.current_ms(),
                scan_count: st.scan_count,
                last_scan_ms: scan.duration_ms,
                process_access: output.access,
                sockets_available: scan.sockets.is_ok(),
                socket_error,
                sockets_skipped: scan.sockets.as_ref().map(|s| s.skipped).unwrap_or_default(),
                persistence_error: st
                    .persistence_error
                    .clone()
                    .or_else(|| lock(&self.persistence_notice).clone()),
                platform: Platform::current(),
            },
            updated_at: now_ms,
        });
        *self.snapshot.write().unwrap_or_else(|p| p.into_inner()) = snapshot.clone();
        snapshot
    }

    /// Forget stopped projects older than the configured retention (hourly).
    fn maybe_prune(&self, st: &mut State, db: &mut Database, now_ms: i64) {
        if now_ms - st.last_prune_ms < PRUNE_INTERVAL_MS {
            return;
        }
        st.last_prune_ms = now_ms;
        let Some(days) = lock(&self.settings).forget_after_days else {
            return;
        };
        let cutoff = now_ms - i64::from(days.max(1)) * DAY_MS;
        let stale: Vec<ProjectId> = st
            .registry
            .values()
            .filter(|e| {
                let r = &e.record;
                r.last_seen_at < cutoff && !r.ignored && !r.favorite
            })
            .map(|e| e.record.id)
            .collect();
        if stale.is_empty() {
            return;
        }
        match db.prune_stale_projects(cutoff) {
            Ok(_) => {
                for id in stale {
                    st.remove_project(id);
                }
            }
            Err(err) => tracing::warn!(%err, "pruning remembered projects failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::http_probe::{HttpProber, ProbeKey, ProbeResult};
    use crate::engine::raw::RawSocket;
    use crate::models::{HttpState, RuntimeStatus};
    use std::fs;
    use tempfile::TempDir;

    struct NoProbe;
    impl HttpProber for NoProbe {
        fn probe(&mut self, _k: &ProbeKey, _t: std::net::SocketAddr, _n: Instant) -> ProbeResult {
            ProbeResult::NotHttp
        }
        fn retain(&mut self, _l: &HashSet<ProbeKey>) {}
    }

    fn engine() -> Engine {
        Engine::with_prober(None, Box::new(NoProbe))
    }

    fn proc(pid: u32, ppid: u32, name: &str, args: &[&str], cwd: &str) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: Some(ppid),
            name: name.to_string(),
            executable: Some(PathBuf::from(format!("/usr/bin/{name}"))),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: Some(PathBuf::from(cwd)),
            cpu_usage: 2.0,
            memory_bytes: 1_000_000,
            started_at: Some(1_700_000_000_000 + i64::from(pid)),
            access: crate::models::AccessLevel::Full,
        }
    }

    fn project_dir(tmp: &TempDir) -> String {
        let root = tmp.path().join("Anılarım app");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"anilarim","scripts":{"dev":"vite"},"devDependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        fs::canonicalize(&root)
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    fn scan(processes: Vec<RawProcess>, sockets: Vec<RawSocket>) -> ScanResult {
        ScanResult {
            processes,
            sockets: Ok(SocketScan {
                sockets,
                skipped: 0,
            }),
            duration_ms: 3,
        }
    }

    fn running_scan(cwd: &str) -> ScanResult {
        scan(
            vec![
                proc(1, 0, "launchd", &[], "/"),
                proc(10, 1, "bun", &["bun", "run", "dev"], cwd),
                proc(11, 10, "node", &["node", "node_modules/.bin/vite"], cwd),
                proc(90, 1, "postgres", &["postgres"], "/opt/pg"),
            ],
            vec![
                RawSocket {
                    address: "127.0.0.1".parse().unwrap(),
                    port: 5173,
                    pids: vec![11],
                },
                RawSocket {
                    address: "127.0.0.1".parse().unwrap(),
                    port: 5432,
                    pids: vec![90],
                },
            ],
        )
    }

    #[test]
    fn discovered_project_is_persisted_shown_and_remembered_after_stopping_and_restart() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let db_path = tmp.path().join("data/localdeck.db");
        let runtime = Runtime::new(Database::open(&db_path).unwrap(), engine()).unwrap();

        let snap = runtime.apply_scan(running_scan(&cwd));
        assert_eq!(snap.projects.len(), 1);
        let project = &snap.projects[0];
        assert_eq!(project.runtime_status, RuntimeStatus::Running);
        assert_eq!(project.name, "Anilarim");
        assert_eq!(project.ports, vec![5173]);
        assert_eq!(project.metrics.process_count, 2);
        assert_eq!(
            project.primary_url.as_ref().unwrap().url,
            "http://localhost:5173"
        );
        let vite_port = snap.ports.iter().find(|p| p.port == 5173).unwrap();
        assert_eq!(vite_port.project_id, Some(project.id));
        assert_eq!(vite_port.http, HttpState::Likely);
        let pg = snap.ports.iter().find(|p| p.port == 5432).unwrap();
        assert_eq!(pg.project_id, None);
        assert_eq!(pg.ownership, PortOwnership::Unassigned);
        assert_eq!(snap.scanner.scan_count, 1);
        assert!(project
            .recent_activity
            .iter()
            .any(|e| e.kind == ActivityKind::ProjectStarted && e.metadata.detected));

        // It stops: two quiet scans, then it is a remembered stopped project.
        runtime.apply_scan(scan(vec![proc(1, 0, "launchd", &[], "/")], vec![]));
        let stopped = runtime.apply_scan(scan(vec![proc(1, 0, "launchd", &[], "/")], vec![]));
        let project = &stopped.projects[0];
        assert_eq!(project.runtime_status, RuntimeStatus::Stopped);
        assert!(project.components.is_empty());
        assert_eq!(
            project.detected_name, "Anilarim",
            "stack fingerprint is kept for stopped projects"
        );
        assert!(project
            .recent_activity
            .iter()
            .any(|e| e.kind == ActivityKind::ProjectStopped));
        assert!(stopped.ports.iter().all(|p| p.port != 5173));

        // New LocalDeck process: the remembered project is still there.
        drop(runtime);
        let reopened = Runtime::new(Database::open(&db_path).unwrap(), engine()).unwrap();
        let snap = reopened.apply_scan(scan(vec![proc(1, 0, "launchd", &[], "/")], vec![]));
        assert_eq!(snap.projects.len(), 1);
        assert_eq!(snap.projects[0].runtime_status, RuntimeStatus::Stopped);
        assert_eq!(snap.projects[0].root_path, cwd);
        assert!(
            snap.projects[0]
                .recent_activity
                .iter()
                .any(|e| e.kind == ActivityKind::ProjectStopped),
            "activity survives restart"
        );
    }

    #[test]
    fn socket_failure_degrades_to_a_ports_diagnostic_but_keeps_projects() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        let mut result = running_scan(&cwd);
        result.sockets = Err(AppError::PortScanFailed("permission denied".into()));
        let snap = runtime.apply_scan(result);
        assert_eq!(snap.projects.len(), 1);
        assert!(snap.ports.is_empty());
        assert!(!snap.scanner.sockets_available);
        assert_eq!(
            snap.scanner.socket_error.as_ref().unwrap().code,
            crate::error::ErrorCode::PortScanFailed
        );
    }

    #[test]
    fn ambiguous_sockets_raise_attention_for_the_involved_project_only() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        let result = scan(
            vec![
                proc(1, 0, "launchd", &[], "/"),
                proc(10, 1, "node", &["node", "server.js"], &cwd),
                proc(91, 1, "mystery", &["mystery"], "/tmp"),
            ],
            vec![RawSocket {
                address: "0.0.0.0".parse().unwrap(),
                port: 3000,
                pids: vec![10, 91],
            }],
        );
        let snap = runtime.apply_scan(result);
        let port = snap.ports.iter().find(|p| p.port == 3000).unwrap();
        assert_eq!(port.ownership, PortOwnership::Ambiguous);
        assert_eq!(port.project_id, None);
        assert_eq!(snap.projects[0].attention.len(), 1);
        assert_eq!(
            snap.projects[0].attention[0].kind,
            AttentionKind::PortConflict
        );
        assert!(
            snap.projects[0].ports.is_empty(),
            "ambiguous ports are not claimed"
        );
    }

    #[test]
    fn database_failure_is_surfaced_and_the_project_is_not_reported_as_discovered() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        lock(&runtime.db)
            .execute_for_tests("DROP TABLE project_overrides")
            .unwrap();
        lock(&runtime.db)
            .execute_for_tests("DROP TABLE activity")
            .unwrap();
        lock(&runtime.db)
            .execute_for_tests("DROP TABLE projects")
            .unwrap();
        let snap = runtime.apply_scan(running_scan(&cwd));
        assert!(snap.projects.is_empty(), "no fabricated remembered state");
        let err = snap.scanner.persistence_error.as_ref().expect("surfaced");
        assert_eq!(err.code, crate::error::ErrorCode::DatabaseError);
        // Ports of the unidentifiable project are not attributed to anything.
        let vite = snap.ports.iter().find(|p| p.port == 5173).unwrap();
        assert_eq!(vite.ownership, PortOwnership::Unassigned);
    }

    #[test]
    fn last_seen_is_throttled_and_snapshot_never_stores_runtime_samples() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        runtime.apply_scan(running_scan(&cwd));
        let id = runtime.snapshot().projects[0].id;
        let first = lock(&runtime.db).get_project(id).unwrap().last_seen_at;
        std::thread::sleep(std::time::Duration::from_millis(15));
        runtime.apply_scan(running_scan(&cwd));
        let second = lock(&runtime.db).get_project(id).unwrap().last_seen_at;
        assert_eq!(first, second, "no write per scan tick");
    }

    #[test]
    fn find_project_reports_structured_not_found() {
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        let err = runtime.find_project(42).unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::ProjectNotFound);
    }

    #[test]
    fn crash_events_become_notifications_only_when_enabled_and_activity_is_listable() {
        let tmp = TempDir::new().unwrap();
        let cwd = project_dir(&tmp);
        let runtime = Runtime::new(Database::open_in_memory().unwrap(), engine()).unwrap();
        runtime.apply_scan(running_scan(&cwd));
        let id = runtime.snapshot().projects[0].id;
        // Starting is not alert-worthy.
        assert!(runtime.drain_notifications().is_empty());
        lock(&runtime.state).pending_events.push(NewActivity {
            project_id: Some(id),
            kind: ActivityKind::ManagedExited,
            metadata: ActivityMetadata {
                exit_code: Some(2),
                ..Default::default()
            },
            created_at: 5,
        });
        runtime.apply_scan(running_scan(&cwd));
        let alerts = runtime.drain_notifications();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "Anilarim");
        assert!(alerts[0].body.contains("code 2"));
        assert!(runtime.drain_notifications().is_empty(), "drained once");

        let mut settings = runtime.settings();
        settings.notifications_enabled = false;
        runtime.update_settings(settings).unwrap();
        lock(&runtime.state).pending_events.push(NewActivity {
            project_id: Some(id),
            kind: ActivityKind::ManagedExited,
            metadata: ActivityMetadata::default(),
            created_at: 6,
        });
        runtime.apply_scan(running_scan(&cwd));
        assert!(runtime.drain_notifications().is_empty());

        let all = runtime.list_activity(None, 100);
        assert!(all.len() >= 3);
        assert!(all.windows(2).all(|w| w[0].id > w[1].id), "newest first");
        assert_eq!(runtime.list_activity(Some(id), 1).len(), 1);
        assert!(runtime.list_activity(Some(id + 100), 10).is_empty());
        runtime.clear_activity().unwrap();
        assert!(runtime.list_activity(None, 10).is_empty());
        assert_eq!(lock(&runtime.db).activity_count().unwrap(), 0);
    }
}
