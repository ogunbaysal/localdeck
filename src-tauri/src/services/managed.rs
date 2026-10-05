//! Starting, restarting and supervising commands that LocalDeck itself spawns:
//! the primary command, launch profiles, optional `PORT`, and bounded
//! auto-restart. Everything is `program + args[] + cwd`, never a shell.

use super::runtime::{lock, ManagedEntry, PendingRestart, Runtime, StartRequest};
use crate::engine::{start_options, STOPPED_STACK_TTL};
use crate::error::{AppError, AppResult};
use crate::models::{
    ActivityKind, ActivityMetadata, AttentionItem, AttentionKind, LaunchProfile,
    LaunchProfileInput, PrimaryCommand, ProjectId, RuntimeStatus,
};
use crate::persistence::NewActivity;
use crate::platform;
use std::time::{Duration, Instant};

const MAX_PROFILES: usize = 8;
const MAX_PROFILE_OPTIONS: usize = 8;
const MAX_PROFILE_NAME: usize = 60;
const RESTART_WAIT: Duration = Duration::from_secs(8);
const RESTART_POLL: Duration = Duration::from_millis(100);

type Env = Vec<(String, String)>;

impl Runtime {
    /// Validate and build launch profiles. Commands come only from option ids
    /// that exist in the project's files right now.
    pub fn build_profiles(
        &self,
        id: ProjectId,
        inputs: &[LaunchProfileInput],
    ) -> AppResult<Vec<LaunchProfile>> {
        if inputs.len() > MAX_PROFILES {
            return Err(AppError::InvalidInput(format!(
                "At most {MAX_PROFILES} launch profiles per project"
            )));
        }
        let existing: Vec<String> = lock(&self.state)
            .registry
            .get(&id)
            .map(|e| {
                e.record
                    .launch_profiles
                    .iter()
                    .map(|p| p.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let base = super::runtime::unix_ms();
        let mut seen_names: Vec<String> = Vec::new();
        let mut used_ids: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let name = input.name.trim().to_string();
            if name.is_empty() || name.chars().count() > MAX_PROFILE_NAME {
                return Err(AppError::InvalidInput(format!(
                    "Profile names must be 1–{MAX_PROFILE_NAME} characters"
                )));
            }
            if seen_names.contains(&name.to_lowercase()) {
                return Err(AppError::InvalidInput(format!(
                    "Duplicate profile name '{name}'"
                )));
            }
            seen_names.push(name.to_lowercase());
            if input.options.is_empty() || input.options.len() > MAX_PROFILE_OPTIONS {
                return Err(AppError::InvalidInput(format!(
                    "A profile needs 1–{MAX_PROFILE_OPTIONS} commands"
                )));
            }
            let mut commands = Vec::new();
            for option in &input.options {
                commands.push(self.derive_command(id, option)?);
            }
            let profile_id = input
                .id
                .clone()
                .filter(|pid| existing.contains(pid) && !used_ids.contains(pid))
                .unwrap_or_else(|| format!("lp-{:x}-{index}", base));
            used_ids.push(profile_id.clone());
            out.push(LaunchProfile {
                id: profile_id,
                name,
                commands,
            });
        }
        Ok(out)
    }

    pub fn set_launch_profiles(
        &self,
        id: ProjectId,
        inputs: Vec<LaunchProfileInput>,
    ) -> AppResult<()> {
        let profiles = self.build_profiles(id, &inputs)?;
        self.with_project_pub(
            id,
            |db| db.set_launch_profiles(id, &profiles),
            |record| record.launch_profiles = profiles.clone(),
        )
    }

    fn note_start_failure(&self, id: ProjectId, error: &AppError) {
        let mut st = lock(&self.state);
        st.push_transient(
            id,
            AttentionItem {
                id: "action-failed".to_string(),
                kind: AttentionKind::ActionFailed,
                message: error.to_string(),
                dismissible: true,
            },
            super::runtime::unix_ms(),
        );
    }

    /// Commands + environment for a start request.
    fn resolve_start(
        &self,
        id: ProjectId,
        profile_id: Option<&str>,
        port: Option<u16>,
    ) -> AppResult<(Vec<PrimaryCommand>, Env)> {
        let (primary, profiles, root) = {
            let st = lock(&self.state);
            let entry = st
                .registry
                .get(&id)
                .ok_or(AppError::ProjectNotFound { id })?;
            (
                entry.record.primary_command.clone(),
                entry.record.launch_profiles.clone(),
                std::path::PathBuf::from(&entry.record.root_path),
            )
        };
        let mut commands = match profile_id {
            Some(pid) => {
                let profile = profiles.iter().find(|p| p.id == pid).ok_or_else(|| {
                    AppError::InvalidInput(format!("Unknown launch profile '{pid}'"))
                })?;
                if profile.commands.is_empty() {
                    return Err(AppError::InvalidInput("The profile has no commands".into()));
                }
                profile.commands.clone()
            }
            None => vec![primary.ok_or(AppError::NoPrimaryCommand)?],
        };
        let mut env = Vec::new();
        if let Some(port) = port {
            if port == 0 {
                return Err(AppError::InvalidInput("Port must be 1–65535".into()));
            }
            let fingerprint =
                lock(&self.engine).stack_for(&root, Instant::now(), STOPPED_STACK_TTL);
            for command in &mut commands {
                let (updated, extra) = start_options::with_port(command, &fingerprint, port);
                *command = updated;
                for pair in extra {
                    if !env.contains(&pair) {
                        env.push(pair);
                    }
                }
            }
        }
        Ok((commands, env))
    }

    /// Spawn every command or none: on a failure the ones already started are killed.
    fn spawn_all(
        &self,
        id: ProjectId,
        commands: Vec<PrimaryCommand>,
        env: Env,
        profile_id: Option<String>,
        port: Option<u16>,
    ) -> AppResult<()> {
        let mut started: Vec<ManagedEntry> = Vec::new();
        for command in commands {
            match platform::spawn_managed(&command, &env) {
                Ok(child) => started.push(ManagedEntry {
                    pid: child.id(),
                    child,
                    command,
                    env: env.clone(),
                    profile_id: profile_id.clone(),
                    started: Instant::now(),
                    stop_requested: false,
                }),
                Err(err) => {
                    for mut entry in started {
                        let _ = entry.child.kill();
                        let _ = entry.child.wait();
                    }
                    self.note_start_failure(id, &err);
                    return Err(err);
                }
            }
        }
        {
            let mut st = lock(&self.state);
            st.cancel_restarts(id);
            st.stop_requests.remove(&id);
            if let Some(items) = st.transient.get_mut(&id) {
                items.retain(|t| {
                    !matches!(
                        t.item.kind,
                        AttentionKind::ManagedExited
                            | AttentionKind::ActionFailed
                            | AttentionKind::AutoRestartGaveUp
                    )
                });
            }
            st.managed.entry(id).or_default().extend(started);
            st.last_start.insert(id, StartRequest { profile_id, port });
        }
        self.request_scan();
        Ok(())
    }

    fn has_live_managed(&self, id: ProjectId) -> bool {
        lock(&self.state)
            .managed
            .get_mut(&id)
            .is_some_and(|entries| {
                entries
                    .iter_mut()
                    .any(|e| e.child.try_wait().ok().flatten().is_none())
            })
    }

    /// Start the primary command (`profile_id: None`) or a launch profile,
    /// optionally on a specific `port`.
    pub fn start_project(
        &self,
        id: ProjectId,
        profile_id: Option<String>,
        port: Option<u16>,
    ) -> AppResult<()> {
        let project = self.find_project(id)?;
        if profile_id.is_none() && project.primary_command.is_none() {
            return Err(AppError::NoPrimaryCommand);
        }
        if project.runtime_status != RuntimeStatus::Stopped || self.has_live_managed(id) {
            return Err(AppError::ProjectAlreadyRunning);
        }
        let (commands, env) = self.resolve_start(id, profile_id.as_deref(), port)?;
        self.spawn_all(id, commands, env, profile_id, port)
    }

    /// Graceful stop, wait for the signalled roots, then start again.
    /// With no explicit request it repeats whatever LocalDeck last started.
    /// Never escalates to force kill: stragglers surface as an explicit error.
    pub fn restart_project(
        &self,
        id: ProjectId,
        profile_id: Option<String>,
        port: Option<u16>,
    ) -> AppResult<()> {
        let project = self.find_project(id)?;
        let (mut profile_id, mut port) = (profile_id, port);
        if profile_id.is_none() && port.is_none() {
            if let Some(last) = lock(&self.state).last_start.get(&id).cloned() {
                profile_id = last
                    .profile_id
                    .filter(|pid| project.launch_profiles.iter().any(|p| &p.id == pid));
                port = last.port;
            }
        }
        if profile_id.is_none() && project.primary_command.is_none() {
            return Err(AppError::NoPrimaryCommand);
        }
        // Validate before stopping anything.
        self.resolve_start(id, profile_id.as_deref(), port)?;
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
                    .filter(|(pid, started)| super::process_service::still_alive(*pid, *started))
                    .count();
                if remaining == 0 {
                    break;
                }
                if Instant::now() >= deadline {
                    let err = AppError::ProcessesStillRunning {
                        remaining: remaining as u32,
                    };
                    self.note_start_failure(id, &err);
                    return Err(err);
                }
                std::thread::sleep(RESTART_POLL);
            }
        }
        let (commands, env) = self.resolve_start(id, profile_id.as_deref(), port)?;
        self.spawn_all(id, commands, env, profile_id, port)
    }

    /// Earliest scheduled auto-restart, if any.
    pub fn next_restart_due(&self) -> Option<Instant> {
        lock(&self.state)
            .restarts
            .values()
            .flat_map(|book| book.pending.iter().map(|p| p.due))
            .min()
    }

    /// Spawn auto-restarts whose delay has elapsed. Called before every scan.
    pub fn run_due_restarts(&self, now: Instant) {
        let due: Vec<(ProjectId, PendingRestart)> = {
            let mut st = lock(&self.state);
            let mut due = Vec::new();
            for (project, book) in st.restarts.iter_mut() {
                let mut index = 0;
                while index < book.pending.len() {
                    if book.pending[index].due <= now {
                        due.push((*project, book.pending.remove(index)));
                    } else {
                        index += 1;
                    }
                }
            }
            due
        };
        let policy = *lock(&self.restart_policy);
        for (project, pending) in due {
            let now_ms = super::runtime::unix_ms();
            match platform::spawn_managed(&pending.command, &pending.env) {
                Ok(child) => {
                    let mut st = lock(&self.state);
                    st.pending_events.push(NewActivity {
                        project_id: Some(project),
                        kind: ActivityKind::AutoRestarted,
                        metadata: ActivityMetadata {
                            attempt: Some(pending.attempt),
                            ..Default::default()
                        },
                        created_at: now_ms,
                    });
                    st.managed.entry(project).or_default().push(ManagedEntry {
                        pid: child.id(),
                        child,
                        command: pending.command,
                        env: pending.env,
                        profile_id: pending.profile_id,
                        started: now,
                        stop_requested: false,
                    });
                }
                Err(_) => {
                    // A failed spawn counts as a failed attempt.
                    let mut st = lock(&self.state);
                    st.plan_restart(project, pending, Duration::ZERO, now, now_ms, &policy);
                }
            }
        }
    }

    /// Next free loopback port at or after `preferred` that nothing in the
    /// current snapshot or the OS holds.
    pub fn suggest_free_port(&self, preferred: u16) -> AppResult<u16> {
        let snapshot = self.snapshot();
        super::free_port::first_free(preferred, |port| {
            snapshot.ports.iter().any(|p| p.port == port) || super::free_port::is_bound(port)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::http_probe::{HttpProber, ProbeKey, ProbeResult};
    use crate::engine::raw::SocketScan;
    use crate::engine::Engine;
    use crate::persistence::Database;
    use crate::services::runtime::{RestartPolicy, ScanResult};
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    struct NoProbe;
    impl HttpProber for NoProbe {
        fn probe(&mut self, _k: &ProbeKey, _t: std::net::SocketAddr, _n: Instant) -> ProbeResult {
            ProbeResult::NotHttp
        }
        fn retain(&mut self, _l: &HashSet<ProbeKey>) {}
    }

    fn setup(tmp: &TempDir) -> (Runtime, ProjectId, PathBuf) {
        let root = tmp.path().join("Çalışma alanı");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"vite","worker":"node w.js"},"devDependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        fs::write(root.join("Makefile"), "serve:\n\tpython3 -m http.server\n").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        let id = db.upsert_project(&root.to_string_lossy(), 1).unwrap().id;
        let runtime = Runtime::new(db, Engine::with_prober(None, Box::new(NoProbe))).unwrap();
        scan(&runtime);
        (runtime, id, root)
    }

    fn scan(runtime: &Runtime) {
        runtime.apply_scan(ScanResult {
            processes: Vec::new(),
            sockets: Ok(SocketScan::default()),
            duration_ms: 1,
        });
    }

    fn cmd(program: &str, args: &[&str], cwd: &Path, id: &str) -> PrimaryCommand {
        PrimaryCommand {
            program: program.into(),
            args: args.iter().map(ToString::to_string).collect(),
            cwd: cwd.to_string_lossy().to_string(),
            label: format!("{program} {}", args.join(" ")),
            script: Some(id.into()),
        }
    }

    fn install_profile(rt: &Runtime, id: ProjectId, commands: Vec<PrimaryCommand>) {
        let mut st = lock(&rt.state);
        let entry = st.registry.get_mut(&id).unwrap();
        entry.record.launch_profiles = vec![LaunchProfile {
            id: "p1".into(),
            name: "stack".into(),
            commands,
        }];
    }

    fn kill_all(rt: &Runtime) {
        let mut st = lock(&rt.state);
        for (_, entries) in st.managed.drain() {
            for mut e in entries {
                let _ = e.child.kill();
                let _ = e.child.wait();
            }
        }
    }

    fn live(rt: &Runtime, id: ProjectId) -> usize {
        lock(&rt.state).managed.get(&id).map_or(0, Vec::len)
    }

    fn input(name: &str, options: &[&str]) -> LaunchProfileInput {
        LaunchProfileInput {
            id: None,
            name: name.into(),
            options: options.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn profiles_are_validated_derived_from_option_ids_and_keep_their_identity() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        rt.set_launch_profiles(
            id,
            vec![
                input("Web + worker", &["pkg:dev", "pkg:worker"]),
                input("Docs", &["make:serve"]),
            ],
        )
        .unwrap();
        let stored = lock(&rt.db).get_project(id).unwrap().launch_profiles;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].commands[0].program, "npm");
        assert_eq!(stored[1].commands[0].program, "make");
        assert_eq!(stored[1].commands[0].cwd, root.to_string_lossy());
        // Updating keeps ids of known profiles, mints new ones otherwise.
        let first_id = stored[0].id.clone();
        rt.set_launch_profiles(
            id,
            vec![
                LaunchProfileInput {
                    id: Some(first_id.clone()),
                    ..input("Renamed", &["pkg:dev"])
                },
                input("New", &["pkg:worker"]),
            ],
        )
        .unwrap();
        let after = lock(&rt.db).get_project(id).unwrap().launch_profiles;
        assert_eq!(after[0].id, first_id);
        assert_ne!(after[1].id, first_id);

        let bad: Vec<(Vec<LaunchProfileInput>, &str)> = vec![
            (
                vec![input("a", &["pkg:dev"]), input("A", &["pkg:dev"])],
                "duplicate name",
            ),
            (vec![input(" ", &["pkg:dev"])], "blank name"),
            (vec![input("x", &[])], "no options"),
            (vec![input("x", &["bash"])], "not an option id"),
            (vec![input("x", &["pkg:nope"])], "unknown option"),
            (vec![input("x", &["make:serve; rm -rf /"])], "injection"),
            (
                (0..9)
                    .map(|i| input(&format!("p{i}"), &["pkg:dev"]))
                    .collect(),
                "too many",
            ),
            (vec![input("x", &["pkg:dev"; 9])], "too many options"),
        ];
        for (profiles, why) in bad {
            let err = rt.set_launch_profiles(id, profiles).unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::InvalidInput, "{why}");
        }
        assert_eq!(
            lock(&rt.db).get_project(id).unwrap().launch_profiles.len(),
            2,
            "failed saves change nothing"
        );
        rt.set_launch_profiles(id, vec![]).unwrap();
        assert!(lock(&rt.db)
            .get_project(id)
            .unwrap()
            .launch_profiles
            .is_empty());
    }

    #[test]
    fn a_profile_starts_every_command_and_blocks_a_second_start() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        install_profile(
            &rt,
            id,
            vec![
                cmd("sleep", &["30"], &root, "pkg:a"),
                cmd("sleep", &["31"], &root, "pkg:b"),
            ],
        );
        rt.start_project(id, Some("p1".into()), None).unwrap();
        assert_eq!(live(&rt, id), 2);
        assert_eq!(
            rt.start_project(id, Some("p1".into()), None)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::ProjectAlreadyRunning
        );
        assert_eq!(
            rt.start_project(id, Some("nope".into()), None)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::ProjectAlreadyRunning
        );
        kill_all(&rt);
    }

    #[test]
    fn a_failing_command_rolls_back_the_ones_already_started() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        install_profile(
            &rt,
            id,
            vec![
                cmd("sleep", &["30"], &root, "pkg:a"),
                cmd("definitely-not-installed-xyz", &[], &root, "pkg:b"),
            ],
        );
        let err = rt.start_project(id, Some("p1".into()), None).unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::CommandSpawnFailed);
        assert_eq!(live(&rt, id), 0, "nothing stays registered");
        scan(&rt);
        assert!(rt
            .find_project(id)
            .unwrap()
            .attention
            .iter()
            .any(|a| a.kind == AttentionKind::ActionFailed));
    }

    #[test]
    fn unknown_profiles_and_missing_primary_commands_are_structured_errors() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        assert_eq!(
            rt.start_project(id, None, None).unwrap_err().code(),
            crate::error::ErrorCode::NoPrimaryCommand
        );
        install_profile(&rt, id, vec![cmd("sleep", &["30"], &root, "pkg:a")]);
        assert_eq!(
            rt.start_project(id, Some("nope".into()), None)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            rt.start_project(id, Some("p1".into()), Some(0))
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn port_reaches_the_child_environment_and_restart_repeats_the_last_request() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        let script = r#"printf %s "$PORT" > port.txt; exec sleep 30"#;
        install_profile(&rt, id, vec![cmd("sh", &["-c", script], &root, "make:x")]);
        rt.start_project(id, Some("p1".into()), Some(4555)).unwrap();
        for _ in 0..50 {
            if root.join("port.txt").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        assert_eq!(fs::read_to_string(root.join("port.txt")).unwrap(), "4555");
        assert_eq!(
            lock(&rt.state).last_start.get(&id).cloned(),
            Some(StartRequest {
                profile_id: Some("p1".into()),
                port: Some(4555)
            })
        );
        kill_all(&rt);
    }

    fn fast_policy(max_attempts: u32) -> RestartPolicy {
        RestartPolicy {
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(40),
            max_attempts,
            reset_after: Duration::from_secs(3600),
        }
    }

    fn crash_project(rt: &Runtime, id: ProjectId, root: &Path) {
        install_profile(rt, id, vec![cmd("sh", &["-c", "exit 7"], root, "pkg:a")]);
        lock(&rt.state)
            .registry
            .get_mut(&id)
            .unwrap()
            .record
            .auto_restart = true;
        rt.start_project(id, Some("p1".into()), None).unwrap();
    }

    fn pump(rt: &Runtime, rounds: usize) {
        for _ in 0..rounds {
            std::thread::sleep(Duration::from_millis(60));
            rt.run_due_restarts(Instant::now());
            scan(rt);
        }
    }

    #[test]
    fn crashing_commands_restart_with_backoff_then_give_up_with_attention() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        rt.set_restart_policy(fast_policy(2));
        crash_project(&rt, id, &root);
        pump(&rt, 25);
        let project = rt.find_project(id).unwrap();
        let kinds: Vec<_> = rt
            .list_activity(Some(id), 100)
            .iter()
            .map(|e| (e.kind, e.metadata.attempt))
            .collect();
        let restarts: Vec<_> = kinds
            .iter()
            .filter(|(k, _)| *k == ActivityKind::AutoRestarted)
            .map(|(_, a)| *a)
            .collect();
        assert_eq!(
            restarts.len(),
            2,
            "exactly max_attempts restarts: {kinds:?}"
        );
        assert!(restarts.contains(&Some(1)) && restarts.contains(&Some(2)));
        assert!(kinds
            .iter()
            .any(|(k, _)| *k == ActivityKind::AutoRestartGaveUp));
        let gave_up = project
            .attention
            .iter()
            .find(|a| a.kind == AttentionKind::AutoRestartGaveUp)
            .expect("attention");
        assert!(gave_up.dismissible && gave_up.message.contains("gave up"));
        assert_eq!(live(&rt, id), 0);
        // Nothing keeps restarting afterwards.
        let before = rt.list_activity(Some(id), 100).len();
        pump(&rt, 6);
        assert_eq!(rt.list_activity(Some(id), 100).len(), before);
    }

    #[test]
    fn without_auto_restart_a_crash_is_only_reported() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        rt.set_restart_policy(fast_policy(3));
        crash_project(&rt, id, &root);
        lock(&rt.state)
            .registry
            .get_mut(&id)
            .unwrap()
            .record
            .auto_restart = false;
        pump(&rt, 8);
        let kinds: Vec<_> = rt
            .list_activity(Some(id), 100)
            .iter()
            .map(|e| e.kind)
            .collect();
        assert!(kinds.contains(&ActivityKind::ManagedExited));
        assert!(!kinds.contains(&ActivityKind::AutoRestarted));
    }

    #[test]
    fn manual_actions_cancel_pending_restarts_and_long_runs_reset_the_counter() {
        let tmp = TempDir::new().unwrap();
        let (rt, id, root) = setup(&tmp);
        rt.set_restart_policy(RestartPolicy {
            base_delay: Duration::from_secs(3600),
            ..fast_policy(5)
        });
        crash_project(&rt, id, &root);
        std::thread::sleep(Duration::from_millis(150));
        scan(&rt);
        assert_eq!(
            lock(&rt.state).restarts.get(&id).map(|b| b.pending.len()),
            Some(1)
        );
        // A manual stop (even of a stopped project) path: starting again clears the book.
        install_profile(&rt, id, vec![cmd("sleep", &["30"], &root, "pkg:a")]);
        rt.start_project(id, Some("p1".into()), None).unwrap();
        assert!(!lock(&rt.state).restarts.contains_key(&id));
        kill_all(&rt);

        // Counter reset: a run longer than reset_after starts counting from 1 again.
        let mut st = lock(&rt.state);
        let policy = RestartPolicy {
            reset_after: Duration::from_secs(10),
            ..fast_policy(2)
        };
        let template = PendingRestart {
            command: cmd("x", &[], &root, "pkg:a"),
            env: vec![],
            profile_id: None,
            due: Instant::now(),
            attempt: 0,
        };
        let clone = |t: &PendingRestart| PendingRestart {
            command: t.command.clone(),
            env: vec![],
            profile_id: None,
            due: t.due,
            attempt: 0,
        };
        st.plan_restart(
            id,
            clone(&template),
            Duration::ZERO,
            Instant::now(),
            0,
            &policy,
        );
        st.plan_restart(
            id,
            clone(&template),
            Duration::ZERO,
            Instant::now(),
            0,
            &policy,
        );
        assert_eq!(st.restarts[&id].attempts, 2);
        st.plan_restart(
            id,
            clone(&template),
            Duration::from_secs(11),
            Instant::now(),
            0,
            &policy,
        );
        assert_eq!(st.restarts[&id].attempts, 1, "healthy run resets");
        st.plan_restart(
            id,
            clone(&template),
            Duration::ZERO,
            Instant::now(),
            0,
            &policy,
        );
        st.plan_restart(
            id,
            clone(&template),
            Duration::ZERO,
            Instant::now(),
            0,
            &policy,
        );
        assert!(st
            .pending_events
            .iter()
            .any(|e| e.kind == ActivityKind::AutoRestartGaveUp));
    }

    #[test]
    fn restart_delay_doubles_and_is_capped() {
        let p = RestartPolicy::default();
        let secs: Vec<u64> = (1..=7).map(|n| p.delay(n).as_secs()).collect();
        assert_eq!(secs, vec![2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn suggest_free_port_skips_ports_the_snapshot_or_os_holds() {
        let tmp = TempDir::new().unwrap();
        let (rt, _, _) = setup(&tmp);
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let suggested = rt.suggest_free_port(port).unwrap();
        assert_ne!(suggested, port);
        assert!(suggested > port);
    }
}
