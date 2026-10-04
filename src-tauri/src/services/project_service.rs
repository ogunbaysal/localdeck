//! Project edits: overrides, flags, scripts and primary command. Every change
//! is written to SQLite first, mirrored into the in-memory registry, and then
//! the scanner is asked for a fresh snapshot.

use super::runtime::{lock, Runtime};
use crate::engine::url::validate_http_url;
use crate::engine::{stack_detector::primary_command_for_script, STOPPED_STACK_TTL};
use crate::error::{AppError, AppResult};
use crate::models::{
    ComponentKind, PackageManager, ProjectId, ProjectOverrides, ProjectScripts, RuntimeStatus,
    Settings,
};
use crate::platform;
use std::path::PathBuf;
use std::time::Instant;

const MAX_NAME_CHARS: usize = 120;
const MAX_RETENTION_DAYS: u32 = 3650;

fn blank_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn validate_editor(id: &Option<String>) -> AppResult<()> {
    if let Some(id) = id {
        if !platform::detect_editors().iter().any(|e| &e.id == id) {
            return Err(AppError::InvalidInput(format!("Unknown editor '{id}'")));
        }
    }
    Ok(())
}

impl Runtime {
    fn with_project<R>(
        &self,
        id: ProjectId,
        edit: impl FnOnce(&mut crate::persistence::Database) -> AppResult<R>,
        mirror: impl FnOnce(&mut crate::persistence::ProjectRecord),
    ) -> AppResult<R> {
        let mut st = lock(&self.state);
        if !st.registry.contains_key(&id) {
            return Err(AppError::ProjectNotFound { id });
        }
        let result = {
            let mut db = lock(&self.db);
            edit(&mut db)?
        };
        if let Some(entry) = st.registry.get_mut(&id) {
            mirror(&mut entry.record);
        }
        drop(st);
        self.request_scan();
        Ok(result)
    }

    pub fn set_project_override(
        &self,
        id: ProjectId,
        overrides: ProjectOverrides,
    ) -> AppResult<()> {
        let name = blank_to_none(overrides.display_name);
        if name
            .as_ref()
            .is_some_and(|n| n.chars().count() > MAX_NAME_CHARS)
        {
            return Err(AppError::InvalidInput(format!(
                "Name must be at most {MAX_NAME_CHARS} characters"
            )));
        }
        let url = match blank_to_none(overrides.primary_url) {
            Some(raw) => Some(validate_http_url(&raw)?),
            None => None,
        };
        let editor = blank_to_none(overrides.preferred_editor);
        validate_editor(&editor)?;
        self.with_project(
            id,
            |db| {
                db.set_display_name(id, name.as_deref())?;
                db.set_primary_url(id, url.as_deref())?;
                db.set_preferred_editor(id, editor.as_deref())
            },
            |record| {
                record.display_name = name.clone();
                record.primary_url = url.clone();
                record.preferred_editor = editor.clone();
            },
        )
    }

    pub fn set_component_kind(
        &self,
        id: ProjectId,
        component_key: &str,
        kind: Option<ComponentKind>,
    ) -> AppResult<()> {
        let key = component_key.trim().to_string();
        self.with_project(
            id,
            |db| db.set_component_kind(id, &key, kind),
            |record| match kind {
                Some(kind) => {
                    record.component_kinds.insert(key.clone(), kind);
                }
                None => {
                    record.component_kinds.remove(&key);
                }
            },
        )
    }

    pub fn set_favorite(&self, id: ProjectId, favorite: bool) -> AppResult<()> {
        self.with_project(
            id,
            |db| db.set_favorite(id, favorite),
            |record| record.favorite = favorite,
        )
    }

    pub fn set_ignored(&self, id: ProjectId, ignored: bool) -> AppResult<()> {
        self.with_project(
            id,
            |db| db.set_ignored(id, ignored),
            |record| record.ignored = ignored,
        )
    }

    fn require_stopped(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        if project.runtime_status != RuntimeStatus::Stopped {
            return Err(AppError::InvalidInput(
                "Stop the project before forgetting it".to_string(),
            ));
        }
        Ok(())
    }

    /// Remove a stopped project (and its overrides/activity) from memory.
    pub fn forget_project(&self, id: ProjectId) -> AppResult<()> {
        self.require_stopped(id)?;
        let mut st = lock(&self.state);
        lock(&self.db).delete_project(id)?;
        st.remove_project(id);
        drop(st);
        self.request_scan();
        Ok(())
    }

    /// Forget every stopped project that is neither ignored nor favourite.
    pub fn forget_stopped_projects(&self) -> AppResult<usize> {
        let snapshot = self.snapshot();
        let ids: Vec<ProjectId> = snapshot
            .projects
            .iter()
            .filter(|p| p.runtime_status == RuntimeStatus::Stopped && !p.ignored && !p.favorite)
            .map(|p| p.id)
            .collect();
        let mut st = lock(&self.state);
        let removed = lock(&self.db).delete_projects(&ids)?;
        for id in ids {
            st.remove_project(id);
        }
        drop(st);
        self.request_scan();
        Ok(removed)
    }

    fn root_of(&self, id: ProjectId) -> AppResult<PathBuf> {
        let st = lock(&self.state);
        st.registry
            .get(&id)
            .map(|e| PathBuf::from(&e.record.root_path))
            .ok_or(AppError::ProjectNotFound { id })
    }

    /// User-requested "refresh detection": drop cached stack/root evidence.
    pub fn refresh_project(&self, id: ProjectId) -> AppResult<()> {
        let root = self.root_of(id)?;
        lock(&self.engine).invalidate(&root);
        self.request_scan();
        Ok(())
    }

    /// `package.json` scripts (read fresh) and the package manager LocalDeck would use.
    pub fn list_project_scripts(&self, id: ProjectId) -> AppResult<ProjectScripts> {
        let root = self.root_of(id)?;
        let fingerprint = {
            let mut engine = lock(&self.engine);
            engine.invalidate(&root);
            engine.stack_for(&root, Instant::now(), STOPPED_STACK_TTL)
        };
        let primary_script = lock(&self.state).registry.get(&id).and_then(|e| {
            e.record
                .primary_command
                .as_ref()
                .and_then(|c| c.script.clone())
        });
        Ok(ProjectScripts {
            package_manager: fingerprint.package_manager.unwrap_or(PackageManager::Npm),
            package_manager_detected: fingerprint.package_manager_detected,
            scripts: fingerprint.scripts.clone(),
            primary_script,
        })
    }

    /// Persist `<package-manager> run <script>` as the project's primary command.
    pub fn set_primary_command(&self, id: ProjectId, script: Option<String>) -> AppResult<()> {
        let command = match blank_to_none(script) {
            None => None,
            Some(script) => {
                let root = self.root_of(id)?;
                let fingerprint = {
                    let mut engine = lock(&self.engine);
                    engine.invalidate(&root);
                    engine.stack_for(&root, Instant::now(), STOPPED_STACK_TTL)
                };
                Some(
                    primary_command_for_script(&root, &fingerprint, &script).ok_or_else(|| {
                        AppError::InvalidInput(format!(
                            "package.json has no script named '{script}'"
                        ))
                    })?,
                )
            }
        };
        self.with_project(
            id,
            |db| db.set_primary_command(id, command.as_ref()),
            |record| record.primary_command = command.clone(),
        )
    }

    pub fn dismiss_attention(&self, id: ProjectId, attention_id: &str) -> AppResult<()> {
        let mut st = lock(&self.state);
        if !st.registry.contains_key(&id) {
            return Err(AppError::ProjectNotFound { id });
        }
        if let Some(items) = st.transient.get_mut(&id) {
            items.retain(|t| t.item.id != attention_id);
        }
        drop(st);
        self.request_scan();
        Ok(())
    }

    pub fn update_settings(&self, settings: Settings) -> AppResult<Settings> {
        let mut settings = settings;
        settings.preferred_editor = blank_to_none(settings.preferred_editor);
        validate_editor(&settings.preferred_editor)?;
        if let Some(days) = settings.forget_after_days {
            if days == 0 || days > MAX_RETENTION_DAYS {
                return Err(AppError::InvalidInput(
                    "Retention must be between 1 and 3650 days".to_string(),
                ));
            }
        }
        lock(&self.db).save_settings(&settings)?;
        *lock(&self.settings) = settings.clone();
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::http_probe::{HttpProber, ProbeKey, ProbeResult};
    use crate::engine::raw::{RawProcess, SocketScan};
    use crate::engine::Engine;
    use crate::persistence::Database;
    use crate::services::runtime::ScanResult;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    struct NoProbe;
    impl HttpProber for NoProbe {
        fn probe(&mut self, _k: &ProbeKey, _t: std::net::SocketAddr, _n: Instant) -> ProbeResult {
            ProbeResult::NotHttp
        }
        fn retain(&mut self, _l: &HashSet<ProbeKey>) {}
    }

    /// Runtime with one remembered (stopped) project backed by a real folder.
    fn runtime_with_project(tmp: &TempDir) -> (Runtime, ProjectId, PathBuf) {
        let root = tmp.path().join("My Project ñ");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"my-project","scripts":{"dev":"vite","build":"vite build"},"devDependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        let record = db.upsert_project(&root.to_string_lossy(), 1_000).unwrap();
        let runtime = Runtime::new(db, Engine::with_prober(None, Box::new(NoProbe))).unwrap();
        runtime.apply_scan(ScanResult {
            processes: Vec::<RawProcess>::new(),
            sockets: Ok(SocketScan::default()),
            duration_ms: 1,
        });
        (runtime, record.id, root)
    }

    fn rescan(runtime: &Runtime) {
        runtime.apply_scan(ScanResult {
            processes: Vec::new(),
            sockets: Ok(SocketScan::default()),
            duration_ms: 1,
        });
    }

    #[test]
    fn overrides_persist_validate_and_clear() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, _) = runtime_with_project(&tmp);
        runtime
            .set_project_override(
                id,
                ProjectOverrides {
                    display_name: Some("  Pretty Name ".into()),
                    primary_url: Some("localhost:4321/app".into()),
                    preferred_editor: None,
                },
            )
            .unwrap();
        rescan(&runtime);
        let project = runtime.find_project(id).unwrap();
        assert_eq!(project.name, "Pretty Name");
        assert_eq!(
            project.overrides.primary_url.as_deref(),
            Some("http://localhost:4321/app")
        );
        assert_eq!(
            project.primary_url.as_ref().unwrap().url,
            "http://localhost:4321/app"
        );
        let stored = lock(&runtime.db).get_project(id).unwrap();
        assert_eq!(stored.display_name.as_deref(), Some("Pretty Name"));

        runtime
            .set_project_override(id, ProjectOverrides::default())
            .unwrap();
        rescan(&runtime);
        let project = runtime.find_project(id).unwrap();
        assert_eq!(project.name, "My Project");
        assert!(project.primary_url.is_none());
    }

    #[test]
    fn invalid_overrides_are_rejected_without_partial_writes() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, _) = runtime_with_project(&tmp);
        for bad in [
            ProjectOverrides {
                display_name: None,
                primary_url: Some("file:///etc/passwd".into()),
                preferred_editor: None,
            },
            ProjectOverrides {
                display_name: None,
                primary_url: None,
                preferred_editor: Some("notepad".into()),
            },
            ProjectOverrides {
                display_name: Some("x".repeat(500)),
                primary_url: None,
                preferred_editor: None,
            },
        ] {
            let err = runtime.set_project_override(id, bad).unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::InvalidInput);
        }
        assert_eq!(
            lock(&runtime.db).get_project(id).unwrap().display_name,
            None
        );
        let missing = runtime
            .set_project_override(999, ProjectOverrides::default())
            .unwrap_err();
        assert_eq!(missing.code(), crate::error::ErrorCode::ProjectNotFound);
    }

    #[test]
    fn primary_command_is_derived_from_a_script_and_survives() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, root) = runtime_with_project(&tmp);
        let scripts = runtime.list_project_scripts(id).unwrap();
        assert_eq!(scripts.package_manager, PackageManager::Bun);
        assert!(scripts.package_manager_detected);
        assert_eq!(scripts.scripts[0].name, "dev");
        assert_eq!(scripts.primary_script, None);

        runtime.set_primary_command(id, Some("dev".into())).unwrap();
        rescan(&runtime);
        let project = runtime.find_project(id).unwrap();
        let command = project.primary_command.unwrap();
        assert_eq!(command.program, "bun");
        assert_eq!(command.args, vec!["run", "dev"]);
        assert_eq!(Path::new(&command.cwd), root);
        let stored = lock(&runtime.db).get_project(id).unwrap();
        assert_eq!(stored.primary_command.unwrap().label, "bun run dev");
        assert_eq!(
            runtime
                .list_project_scripts(id)
                .unwrap()
                .primary_script
                .as_deref(),
            Some("dev")
        );

        let err = runtime
            .set_primary_command(id, Some("nope".into()))
            .unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::InvalidInput);
        runtime.set_primary_command(id, None).unwrap();
        assert!(lock(&runtime.db)
            .get_project(id)
            .unwrap()
            .primary_command
            .is_none());
    }

    #[test]
    fn ignore_favorite_component_kind_and_forget() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, _) = runtime_with_project(&tmp);
        runtime.set_ignored(id, true).unwrap();
        runtime.set_favorite(id, true).unwrap();
        runtime
            .set_component_kind(id, "vite@.", Some(ComponentKind::Api))
            .unwrap();
        rescan(&runtime);
        let project = runtime.find_project(id).unwrap();
        assert!(
            project.ignored && project.favorite,
            "ignored projects stay in the snapshot, flagged"
        );
        let stored = lock(&runtime.db).get_project(id).unwrap();
        assert_eq!(stored.component_kinds["vite@."], ComponentKind::Api);

        // Bulk forget skips ignored/favourite projects.
        assert_eq!(runtime.forget_stopped_projects().unwrap(), 0);
        runtime.set_ignored(id, false).unwrap();
        runtime.set_favorite(id, false).unwrap();
        rescan(&runtime);
        runtime.forget_project(id).unwrap();
        rescan(&runtime);
        assert!(runtime.snapshot().projects.is_empty());
        assert!(lock(&runtime.db).list_projects().unwrap().is_empty());
    }

    #[test]
    fn settings_are_validated_and_persisted() {
        let tmp = TempDir::new().unwrap();
        let (runtime, _, _) = runtime_with_project(&tmp);
        let mut settings = runtime.settings();
        settings.forget_after_days = Some(30);
        settings.theme = crate::models::Theme::Dark;
        let saved = runtime.update_settings(settings.clone()).unwrap();
        assert_eq!(saved.forget_after_days, Some(30));
        assert_eq!(
            lock(&runtime.db).load_settings().unwrap().theme,
            crate::models::Theme::Dark
        );
        settings.forget_after_days = Some(0);
        assert!(runtime.update_settings(settings.clone()).is_err());
        settings.forget_after_days = None;
        settings.preferred_editor = Some("emacs-gui".into());
        assert!(runtime.update_settings(settings).is_err());
    }

    #[test]
    fn dismissing_unknown_attention_is_harmless_and_unknown_project_errors() {
        let tmp = TempDir::new().unwrap();
        let (runtime, id, _) = runtime_with_project(&tmp);
        runtime.dismiss_attention(id, "nothing").unwrap();
        assert_eq!(
            runtime.dismiss_attention(777, "x").unwrap_err().code(),
            crate::error::ErrorCode::ProjectNotFound
        );
    }
}
