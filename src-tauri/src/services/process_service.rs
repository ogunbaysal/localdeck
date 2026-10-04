//! Process actions: graceful terminate vs. explicit force kill, project
//! stop/start/restart, and the managed-process registry.
//!
//! Safety: only PIDs shown in the latest snapshot can be signalled, and the
//! live process is re-checked (existence + start time) right before signalling
//! so a recycled PID is never hit.

use super::runtime::{lock, ManagedEntry, Runtime};
use crate::error::{AppError, AppResult};
use crate::models::{
    AttentionItem, AttentionKind, ProcessFailure, ProjectId, RuntimeStatus, StopReport,
};
use crate::platform::{self, Signal};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const RESTART_WAIT: Duration = Duration::from_secs(8);
const RESTART_POLL: Duration = Duration::from_millis(100);

/// Start time (epoch ms) of the live process `pid`, if it exists.
pub fn live_start_time(pid: u32) -> Option<i64> {
    let mut system = System::new();
    let target = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(target)
        .map(|p| (p.start_time() as i64).saturating_mul(1000))
}

/// Is `pid` still the same process (existence + start time)?
pub fn still_alive(pid: u32, started_at: Option<i64>) -> bool {
    match live_start_time(pid) {
        None => false,
        Some(live) => started_at.is_none_or(|expected| expected == live),
    }
}

fn signal_checked(pid: u32, expected_start: Option<i64>, signal: Signal) -> AppResult<()> {
    if pid == std::process::id() {
        return Err(AppError::InvalidInput(
            "LocalDeck cannot signal its own process".to_string(),
        ));
    }
    let Some(live) = live_start_time(pid) else {
        return Err(AppError::ProcessNotFound { pid });
    };
    if let Some(expected) = expected_start {
        if expected != live {
            // The PID now belongs to a different process.
            return Err(AppError::ProcessNotFound { pid });
        }
    }
    platform::send_signal(pid, signal)
}

impl Runtime {
    fn note_action_failure(&self, id: ProjectId, error: &AppError) {
        let now = super::runtime::unix_ms();
        let mut st = lock(&self.state);
        st.push_transient(
            id,
            AttentionItem {
                id: "action-failed".to_string(),
                kind: AttentionKind::ActionFailed,
                message: error.to_string(),
                dismissible: true,
            },
            now,
        );
    }

    /// Terminate (SIGTERM) or force-kill (SIGKILL) one process shown in the UI.
    pub fn signal_process(&self, pid: u32, started_at: Option<i64>, force: bool) -> AppResult<()> {
        let known = lock(&self.state).known_processes.get(&pid).copied();
        let Some(known_start) = known else {
            return Err(AppError::ProcessNotFound { pid });
        };
        if let (Some(requested), Some(known)) = (started_at, known_start) {
            if requested != known {
                return Err(AppError::ProcessNotFound { pid });
            }
        }
        let signal = if force {
            Signal::Kill
        } else {
            Signal::Terminate
        };
        let result = signal_checked(pid, started_at.or(known_start), signal);
        if result.is_ok() {
            self.request_scan();
        }
        result
    }

    /// Signal the root process of each of the project's components.
    /// Graceful by default; `force` is the separate, explicit SIGKILL path.
    pub fn stop_project(&self, id: ProjectId, force: bool) -> AppResult<StopReport> {
        let project = self.find_project(id)?;
        if project.runtime_status == RuntimeStatus::Stopped {
            return Ok(StopReport::default());
        }
        let signal = if force {
            Signal::Kill
        } else {
            Signal::Terminate
        };
        let mut report = StopReport::default();
        let mut errors: Vec<AppError> = Vec::new();
        let mut requested: Vec<(u32, Option<i64>)> = Vec::new();
        for component in &project.components {
            let started = project
                .processes
                .iter()
                .find(|p| p.pid == component.root_pid)
                .and_then(|p| p.started_at);
            match signal_checked(component.root_pid, started, signal) {
                Ok(()) => {
                    report.signaled.push(component.root_pid);
                    requested.push((component.root_pid, started));
                }
                // Already gone between click and signal: nothing to stop.
                Err(AppError::ProcessNotFound { .. }) => {}
                Err(err) => {
                    report.failed.push(ProcessFailure {
                        pid: component.root_pid,
                        error: err.payload(),
                    });
                    errors.push(err);
                }
            }
        }
        {
            let mut st = lock(&self.state);
            if let Some(entry) = st.managed.get_mut(&id) {
                entry.stop_requested = true;
            }
            if !requested.is_empty() {
                st.stop_requests.insert(id, requested);
            }
            if let Some(items) = st.transient.get_mut(&id) {
                items.retain(|t| t.item.kind != AttentionKind::ManagedExited);
            }
        }
        self.request_scan();
        if report.signaled.is_empty() && !errors.is_empty() {
            let first = errors.remove(0);
            self.note_action_failure(id, &first);
            return Err(first);
        }
        Ok(report)
    }

    fn spawn_for(&self, id: ProjectId) -> AppResult<()> {
        let command = {
            let st = lock(&self.state);
            st.registry
                .get(&id)
                .ok_or(AppError::ProjectNotFound { id })?
                .record
                .primary_command
                .clone()
        }
        .ok_or(AppError::NoPrimaryCommand)?;

        // Blocking: may resolve the user's login-shell PATH on first use.
        let child = match platform::spawn_managed(&command) {
            Ok(child) => child,
            Err(err) => {
                self.note_action_failure(id, &err);
                return Err(err);
            }
        };
        let pid = child.id();
        {
            let mut st = lock(&self.state);
            if let Some(mut previous) = st.managed.remove(&id) {
                let _ = previous.child.try_wait();
            }
            st.stop_requests.remove(&id);
            if let Some(items) = st.transient.get_mut(&id) {
                items.retain(|t| {
                    !matches!(
                        t.item.kind,
                        AttentionKind::ManagedExited | AttentionKind::ActionFailed
                    )
                });
            }
            st.managed.insert(
                id,
                ManagedEntry {
                    child,
                    pid,
                    label: command.label.clone(),
                    stop_requested: false,
                },
            );
        }
        self.request_scan();
        Ok(())
    }

    /// Spawn the project's primary command (`program + args + cwd`, no shell).
    pub fn start_project(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        if project.primary_command.is_none() {
            return Err(AppError::NoPrimaryCommand);
        }
        if project.runtime_status != RuntimeStatus::Stopped {
            return Err(AppError::ProjectAlreadyRunning);
        }
        let still_managed = lock(&self.state)
            .managed
            .get_mut(&id)
            .is_some_and(|m| m.child.try_wait().ok().flatten().is_none());
        if still_managed {
            return Err(AppError::ProjectAlreadyRunning);
        }
        self.spawn_for(id)
    }

    /// Graceful stop, wait for the signalled roots, then a managed start.
    /// Never escalates to force kill: stragglers surface as an explicit error.
    pub fn restart_project(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        if project.primary_command.is_none() {
            return Err(AppError::NoPrimaryCommand);
        }
        if project.runtime_status != RuntimeStatus::Stopped {
            let report = self.stop_project(id, false)?;
            let pending: Vec<(u32, Option<i64>)> = report
                .signaled
                .iter()
                .map(|pid| {
                    let started = project
                        .processes
                        .iter()
                        .find(|p| p.pid == *pid)
                        .and_then(|p| p.started_at);
                    (*pid, started)
                })
                .collect();
            let deadline = Instant::now() + RESTART_WAIT;
            loop {
                let remaining = pending
                    .iter()
                    .filter(|(pid, started)| still_alive(*pid, *started))
                    .count();
                if remaining == 0 {
                    break;
                }
                if Instant::now() >= deadline {
                    let err = AppError::ProcessesStillRunning {
                        remaining: remaining as u32,
                    };
                    self.note_action_failure(id, &err);
                    return Err(err);
                }
                std::thread::sleep(RESTART_POLL);
            }
        }
        self.spawn_for(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::http_probe::{HttpProber, ProbeKey, ProbeResult};
    use crate::engine::providers::{NetstatSocketProvider, SysinfoProcessProvider};
    use crate::engine::raw::{ProcessProvider, SocketProvider};
    use crate::engine::Engine;
    use crate::models::PrimaryCommand;
    use crate::persistence::Database;
    use crate::services::runtime::ScanResult;
    use std::collections::HashSet;
    use std::fs;
    use std::process::{Child, Command};
    use tempfile::TempDir;

    struct NoProbe;
    impl HttpProber for NoProbe {
        fn probe(&mut self, _k: &ProbeKey, _t: std::net::SocketAddr, _n: Instant) -> ProbeResult {
            ProbeResult::NotHttp
        }
        fn retain(&mut self, _l: &HashSet<ProbeKey>) {}
    }

    /// Real scan against the real OS for a project folder that exists on disk.
    fn real_scan(runtime: &Runtime, procs: &mut SysinfoProcessProvider) {
        let processes = procs.snapshot().unwrap();
        let sockets = NetstatSocketProvider.listening_sockets();
        runtime.apply_scan(ScanResult {
            processes,
            sockets,
            duration_ms: 1,
        });
    }

    fn make_runtime(
        tmp: &TempDir,
        command: Option<PrimaryCommand>,
    ) -> (Runtime, ProjectId, std::path::PathBuf) {
        let root = tmp.path().join("proj with space");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        let record = db.upsert_project(&root.to_string_lossy(), 1).unwrap();
        if let Some(command) = &command {
            db.set_primary_command(record.id, Some(command)).unwrap();
        }
        let runtime = Runtime::new(db, Engine::with_prober(None, Box::new(NoProbe))).unwrap();
        (runtime, record.id, root)
    }

    fn sleep_command(root: &std::path::Path) -> PrimaryCommand {
        PrimaryCommand {
            program: "sleep".into(),
            args: vec!["60".into()],
            cwd: root.to_string_lossy().to_string(),
            label: "sleep 60".into(),
            script: None,
        }
    }

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..100 {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn kill_leftovers(runtime: &Runtime) {
        let mut st = lock(&runtime.state);
        for (_, mut entry) in st.managed.drain() {
            let _ = entry.child.kill();
            let _ = entry.child.wait();
        }
    }

    #[test]
    fn start_stop_restart_a_managed_project_end_to_end() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, root) = make_runtime(&tmp, None);
        let mut procs = SysinfoProcessProvider::new();
        real_scan(&runtime, &mut procs);

        // No primary command → explicit structured error.
        assert_eq!(
            runtime.start_project(id).unwrap_err().code(),
            crate::error::ErrorCode::NoPrimaryCommand
        );
        assert_eq!(
            runtime.restart_project(id).unwrap_err().code(),
            crate::error::ErrorCode::NoPrimaryCommand
        );

        // Persist a primary command (sleep stands in for a dev server).
        lock(&runtime.db)
            .set_primary_command(id, Some(&sleep_command(&root)))
            .unwrap();
        lock(&runtime.state)
            .registry
            .get_mut(&id)
            .unwrap()
            .record
            .primary_command = Some(sleep_command(&root));
        real_scan(&runtime, &mut procs);

        runtime.start_project(id).expect("managed start");
        assert!(
            wait_until(|| {
                real_scan(&runtime, &mut procs);
                runtime.find_project(id).unwrap().runtime_status != RuntimeStatus::Stopped
            }),
            "project becomes active"
        );
        let project = runtime.find_project(id).unwrap();
        assert_eq!(project.components.len(), 1);
        assert_eq!(
            project.components[0].origin,
            crate::models::ProcessOrigin::LocalDeckManaged
        );
        assert!(project
            .processes
            .iter()
            .all(|p| p.origin == crate::models::ProcessOrigin::LocalDeckManaged));
        assert_eq!(
            runtime.start_project(id).unwrap_err().code(),
            crate::error::ErrorCode::ProjectAlreadyRunning
        );

        // Restart: graceful stop → new managed process with a different pid.
        let old_pid = project.components[0].root_pid;
        runtime.restart_project(id).expect("restart");
        assert!(
            wait_until(|| {
                real_scan(&runtime, &mut procs);
                let p = runtime.find_project(id).unwrap();
                p.runtime_status != RuntimeStatus::Stopped
                    && p.components.iter().all(|c| c.root_pid != old_pid)
            }),
            "a fresh process replaced the old one"
        );
        assert!(!still_alive(old_pid, None));

        // Graceful stop: signalled, exit is expected (no attention).
        let current = runtime.find_project(id).unwrap();
        let report = runtime.stop_project(id, false).unwrap();
        assert_eq!(report.signaled, vec![current.components[0].root_pid]);
        assert!(report.failed.is_empty());
        assert!(wait_until(|| {
            real_scan(&runtime, &mut procs);
            real_scan(&runtime, &mut procs);
            runtime.find_project(id).unwrap().runtime_status == RuntimeStatus::Stopped
        }));
        let stopped = runtime.find_project(id).unwrap();
        assert!(
            stopped.attention.is_empty(),
            "an expected stop is not an attention event: {:?}",
            stopped.attention
        );
        kill_leftovers(&runtime);
    }

    #[test]
    fn unexpected_managed_exit_raises_attention_and_activity() {
        let tmp = TempDir::new().unwrap();
        let command = PrimaryCommand {
            program: "sh".into(),
            args: vec!["-c".into(), "exit 3".into()],
            cwd: String::new(),
            label: "failing dev".into(),
            script: None,
        };
        let (runtime, id, root) = make_runtime(&tmp, None);
        let command = PrimaryCommand {
            cwd: root.to_string_lossy().to_string(),
            ..command
        };
        lock(&runtime.state)
            .registry
            .get_mut(&id)
            .unwrap()
            .record
            .primary_command = Some(command);
        let mut procs = SysinfoProcessProvider::new();
        real_scan(&runtime, &mut procs);
        runtime.start_project(id).unwrap();
        assert!(wait_until(|| {
            real_scan(&runtime, &mut procs);
            runtime
                .find_project(id)
                .unwrap()
                .attention
                .iter()
                .any(|a| a.kind == AttentionKind::ManagedExited)
        }));
        let project = runtime.find_project(id).unwrap();
        let item = project
            .attention
            .iter()
            .find(|a| a.kind == AttentionKind::ManagedExited)
            .unwrap();
        assert!(item.message.contains("code 3") && item.message.contains("failing dev"));
        assert!(item.dismissible);
        assert!(project
            .recent_activity
            .iter()
            .any(|e| e.kind == crate::models::ActivityKind::ManagedExited
                && e.metadata.exit_code == Some(3)));
        runtime.dismiss_attention(id, "managed-exited").unwrap();
        real_scan(&runtime, &mut procs);
        assert!(runtime.find_project(id).unwrap().attention.is_empty());
    }

    #[test]
    fn spawn_failure_is_reported_structurally_and_flagged() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, root) = make_runtime(&tmp, None);
        let command = PrimaryCommand {
            program: "definitely-not-installed-xyz".into(),
            args: vec![],
            cwd: root.to_string_lossy().to_string(),
            label: "missing".into(),
            script: None,
        };
        lock(&runtime.state)
            .registry
            .get_mut(&id)
            .unwrap()
            .record
            .primary_command = Some(command);
        let mut procs = SysinfoProcessProvider::new();
        real_scan(&runtime, &mut procs);
        let err = runtime.start_project(id).unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::CommandSpawnFailed);
        real_scan(&runtime, &mut procs);
        let project = runtime.find_project(id).unwrap();
        assert!(project
            .attention
            .iter()
            .any(|a| a.kind == AttentionKind::ActionFailed));
    }

    fn spawn_in(dir: &std::path::Path, program: &str, args: &[&str]) -> Child {
        Command::new(program)
            .args(args)
            .current_dir(dir)
            .spawn()
            .unwrap()
    }

    #[test]
    fn terminate_requires_a_displayed_process_and_checks_start_time() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, root) = make_runtime(&tmp, None);
        let mut child = spawn_in(&root, "sleep", &["60"]);
        let pid = child.id();
        let mut procs = SysinfoProcessProvider::new();
        real_scan(&runtime, &mut procs);
        let project = runtime.find_project(id).unwrap();
        let shown = project
            .processes
            .iter()
            .find(|p| p.pid == pid)
            .expect("sleep is attributed by cwd");

        // Unknown pid (not in the snapshot) is refused outright.
        assert_eq!(
            runtime.signal_process(1, None, false).unwrap_err().code(),
            crate::error::ErrorCode::ProcessNotFound
        );
        // Wrong start time → treated as PID reuse.
        let stale = shown.started_at.map(|s| s + 5_000);
        assert_eq!(
            runtime
                .signal_process(pid, stale, false)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::ProcessNotFound
        );
        assert!(still_alive(pid, None), "nothing was signalled");

        runtime
            .signal_process(pid, shown.started_at, false)
            .expect("graceful terminate");
        let status = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGTERM));
        // Gone between click and action → structured not-found.
        assert_eq!(
            runtime
                .signal_process(pid, shown.started_at, true)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::ProcessNotFound
        );
    }

    #[test]
    fn force_kill_is_a_separate_explicit_path() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, root) = make_runtime(&tmp, None);
        // A process that ignores SIGTERM survives graceful stop and needs Force Stop.
        let mut child = spawn_in(
            &root,
            "sh",
            &["-c", "trap '' TERM; while true; do sleep 1; done"],
        );
        let mut procs = SysinfoProcessProvider::new();
        assert!(wait_until(|| {
            real_scan(&runtime, &mut procs);
            runtime.find_project(id).unwrap().runtime_status != RuntimeStatus::Stopped
        }));
        let report = runtime.stop_project(id, false).unwrap();
        assert_eq!(report.signaled.len(), 1);
        std::thread::sleep(Duration::from_millis(600));
        real_scan(&runtime, &mut procs);
        let project = runtime.find_project(id).unwrap();
        assert_ne!(
            project.runtime_status,
            RuntimeStatus::Stopped,
            "no silent escalation"
        );
        assert!(
            project.stop_pending,
            "UI is told a graceful stop is still pending"
        );

        let report = runtime.stop_project(id, true).unwrap();
        assert_eq!(report.signaled.len(), 1);
        let status = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    #[test]
    fn stopping_a_stopped_project_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, _) = make_runtime(&tmp, None);
        let mut procs = SysinfoProcessProvider::new();
        real_scan(&runtime, &mut procs);
        assert_eq!(
            runtime.stop_project(id, false).unwrap(),
            StopReport::default()
        );
    }
}
