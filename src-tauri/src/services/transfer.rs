//! Manual "Add project" and export/import of overrides.
//!
//! Exported files reference commands by *option id only*; importing re-derives
//! every command from the project's own files, so a file can never inject a
//! program, arguments or a working directory.

use super::runtime::{lock, unix_ms, Runtime};
use crate::engine::project_detector::{canonicalize_lenient, detect_root, DetectorConfig};
use crate::engine::url::validate_http_url;
use crate::error::{AppError, AppResult};
use crate::models::{
    ComponentKind, ImportReport, ImportSkip, LaunchProfileInput, ProjectId, ProjectOverrides,
    RootKind, Theme,
};
use crate::platform;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;
const FILE_VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileSettings {
    theme: Option<Theme>,
    preferred_editor: Option<String>,
    preferred_terminal: Option<String>,
    forget_after_days: Option<u32>,
    notifications_enabled: Option<bool>,
    global_shortcut: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileProfile {
    name: String,
    options: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileProject {
    root_path: String,
    display_name: Option<String>,
    primary_url: Option<String>,
    preferred_editor: Option<String>,
    preferred_terminal: Option<String>,
    auto_restart: bool,
    favorite: bool,
    ignored: bool,
    component_kinds: BTreeMap<String, ComponentKind>,
    primary_option: Option<String>,
    launch_profiles: Vec<FileProfile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    version: u32,
    #[serde(default)]
    exported_at: i64,
    #[serde(default)]
    settings: FileSettings,
    #[serde(default)]
    projects: Vec<FileProject>,
}

fn skip(subject: impl Into<String>, reason: impl Into<String>) -> ImportSkip {
    ImportSkip {
        subject: subject.into(),
        reason: reason.into(),
    }
}

fn detector_config() -> DetectorConfig {
    DetectorConfig::new(std::env::var_os("HOME").map(PathBuf::from))
}

impl Runtime {
    /// Remember `path` as a project (resolved to its project root when it has
    /// one). Idempotent. The path comes from a native dialog, never from React.
    pub fn add_project_path(&self, path: &Path) -> AppResult<ProjectId> {
        if !path.is_dir() {
            return Err(AppError::PathNotFound(path.display().to_string()));
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AppError::PathNotFound(path.display().to_string()))?;
        let config = detector_config();
        if config.is_forbidden_root(&canonical) {
            return Err(AppError::InvalidInput(
                "That folder is your home directory or a system location; choose a project folder"
                    .to_string(),
            ));
        }
        let (root, kind) = match detect_root(&canonical, &config) {
            Some(found) => (found.path, found.kind),
            None => (canonical, RootKind::Manifest),
        };
        self.remember_root(&root, kind)
    }

    fn remember_root(&self, root: &Path, kind: RootKind) -> AppResult<ProjectId> {
        let key = root.to_string_lossy().to_string();
        let mut st = lock(&self.state);
        if let Some(id) = st.by_path.get(&key).copied() {
            return Ok(id);
        }
        let record = lock(&self.db).upsert_project(&key, unix_ms())?;
        let id = record.id;
        st.insert_record(record, kind);
        drop(st);
        self.request_scan();
        Ok(id)
    }

    /// Overrides as pretty JSON. Commands are exported as option ids only.
    pub fn export_overrides_json(&self) -> AppResult<String> {
        let settings = self.settings();
        let st = lock(&self.state);
        let mut projects: Vec<FileProject> = st
            .registry
            .values()
            .map(|entry| {
                let r = &entry.record;
                FileProject {
                    root_path: r.root_path.clone(),
                    display_name: r.display_name.clone(),
                    primary_url: r.primary_url.clone(),
                    preferred_editor: r.preferred_editor.clone(),
                    preferred_terminal: r.preferred_terminal.clone(),
                    auto_restart: r.auto_restart,
                    favorite: r.favorite,
                    ignored: r.ignored,
                    component_kinds: r.component_kinds.clone(),
                    primary_option: r.primary_command.as_ref().and_then(|c| {
                        c.script.as_ref().map(|s| {
                            if s.contains(':') {
                                s.clone()
                            } else {
                                format!("pkg:{s}")
                            }
                        })
                    }),
                    launch_profiles: r
                        .launch_profiles
                        .iter()
                        .map(|p| FileProfile {
                            name: p.name.clone(),
                            options: p.commands.iter().filter_map(|c| c.script.clone()).collect(),
                        })
                        .collect(),
                }
            })
            .collect();
        projects.sort_by(|a, b| a.root_path.cmp(&b.root_path));
        let file = ExportFile {
            version: FILE_VERSION,
            exported_at: unix_ms(),
            settings: FileSettings {
                theme: Some(settings.theme),
                preferred_editor: settings.preferred_editor,
                preferred_terminal: settings.preferred_terminal,
                forget_after_days: settings.forget_after_days,
                notifications_enabled: Some(settings.notifications_enabled),
                global_shortcut: settings.global_shortcut,
            },
            projects,
        };
        serde_json::to_string_pretty(&file)
            .map_err(|e| AppError::Internal(format!("could not encode export: {e}")))
    }

    /// Apply an overrides file. Every entry is validated like a normal edit;
    /// anything that does not validate is skipped and reported.
    pub fn import_overrides_json(&self, json: &str) -> AppResult<ImportReport> {
        if json.len() > MAX_IMPORT_BYTES {
            return Err(AppError::InvalidInput(
                "The file is too large to be an overrides export".to_string(),
            ));
        }
        let file: ExportFile = serde_json::from_str(json)
            .map_err(|e| AppError::InvalidInput(format!("Not a LocalDeck overrides file: {e}")))?;
        if file.version != FILE_VERSION {
            return Err(AppError::InvalidInput(format!(
                "Unsupported overrides file version {}",
                file.version
            )));
        }
        let mut report = ImportReport::default();
        self.import_settings(&file.settings, &mut report);
        let editors = platform::detect_editors();
        let terminals = platform::detect_terminals();
        for project in &file.projects {
            match self.import_project(project, &editors, &terminals, &mut report.skipped) {
                Ok(()) => report.applied += 1,
                Err(reason) => report.skipped.push(skip(&project.root_path, reason)),
            }
        }
        Ok(report)
    }

    fn import_settings(&self, file: &FileSettings, report: &mut ImportReport) {
        let current = self.settings();
        let mut next = current.clone();
        if let Some(theme) = file.theme {
            next.theme = theme;
        }
        if file.preferred_editor.is_some() {
            next.preferred_editor = file.preferred_editor.clone();
        }
        if file.preferred_terminal.is_some() {
            next.preferred_terminal = file.preferred_terminal.clone();
        }
        if file.forget_after_days.is_some() {
            next.forget_after_days = file.forget_after_days;
        }
        if let Some(enabled) = file.notifications_enabled {
            next.notifications_enabled = enabled;
        }
        if file.global_shortcut.is_some() {
            next.global_shortcut = file.global_shortcut.clone();
        }
        // `launchAtLogin` is never imported.
        next.launch_at_login = current.launch_at_login;
        if next == current {
            return;
        }
        match self.update_settings(next) {
            Ok(_) => report.settings_applied = true,
            Err(err) => report.skipped.push(skip("settings", err.to_string())),
        }
    }

    fn import_project(
        &self,
        project: &FileProject,
        editors: &[crate::models::EditorInfo],
        terminals: &[crate::models::TerminalInfo],
        skipped: &mut Vec<ImportSkip>,
    ) -> Result<(), String> {
        let subject = project.root_path.as_str();
        let path = Path::new(subject);
        if !path.is_absolute() || !path.is_dir() {
            return Err("folder does not exist on this machine".to_string());
        }
        let canonical = canonicalize_lenient(path);
        if detector_config().is_forbidden_root(&canonical) {
            return Err("not an allowed project location".to_string());
        }
        let kind = detect_root(&canonical, &detector_config())
            .map(|r| r.kind)
            .unwrap_or(RootKind::Manifest);
        let id = self
            .remember_root(&canonical, kind)
            .map_err(|e| e.to_string())?;

        let mut note =
            |what: &str, why: String| skipped.push(skip(subject, format!("{what}: {why}")));
        let primary_url = match project
            .primary_url
            .as_deref()
            .filter(|u| !u.trim().is_empty())
        {
            Some(raw) => match validate_http_url(raw) {
                Ok(url) => Some(url),
                Err(e) => {
                    note("primary URL", e.to_string());
                    None
                }
            },
            None => None,
        };
        let preferred_editor = project.preferred_editor.clone().filter(|id| {
            let known = editors.iter().any(|e| &e.id == id);
            if !known {
                note("editor", format!("unknown editor '{id}'"));
            }
            known
        });
        let preferred_terminal = project.preferred_terminal.clone().filter(|id| {
            let known = terminals.iter().any(|t| &t.id == id);
            if !known {
                note("terminal", format!("unknown terminal '{id}'"));
            }
            known
        });
        self.set_project_override(
            id,
            ProjectOverrides {
                display_name: project.display_name.clone(),
                primary_url,
                preferred_editor,
                preferred_terminal,
                auto_restart: project.auto_restart,
            },
        )
        .map_err(|e| e.to_string())?;
        self.set_favorite(id, project.favorite)
            .map_err(|e| e.to_string())?;
        self.set_ignored(id, project.ignored)
            .map_err(|e| e.to_string())?;
        for (key, kind) in &project.component_kinds {
            if let Err(e) = self.set_component_kind(id, key, Some(*kind)) {
                note("component kind", e.to_string());
            }
        }
        if let Some(option) = &project.primary_option {
            if let Err(e) = self.set_primary_command(id, Some(option.clone())) {
                note("primary command", e.to_string());
            }
        }
        if !project.launch_profiles.is_empty() {
            let inputs: Vec<LaunchProfileInput> = project
                .launch_profiles
                .iter()
                .map(|p| LaunchProfileInput {
                    id: None,
                    name: p.name.clone(),
                    options: p.options.clone(),
                })
                .collect();
            if let Err(e) = self.set_launch_profiles(id, inputs) {
                note("launch profiles", e.to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::http_probe::{HttpProber, ProbeKey, ProbeResult};
    use crate::engine::Engine;
    use crate::persistence::Database;
    use std::collections::HashSet;
    use std::fs;
    use std::time::Instant;
    use tempfile::TempDir;

    struct NoProbe;
    impl HttpProber for NoProbe {
        fn probe(&mut self, _k: &ProbeKey, _t: std::net::SocketAddr, _n: Instant) -> ProbeResult {
            ProbeResult::NotHttp
        }
        fn retain(&mut self, _l: &HashSet<ProbeKey>) {}
    }

    fn runtime() -> Runtime {
        Runtime::new(
            Database::open_in_memory().unwrap(),
            Engine::with_prober(None, Box::new(NoProbe)),
        )
        .unwrap()
    }

    fn project_dir(base: &Path, name: &str) -> PathBuf {
        let root = base.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"vite","worker":"node w.js"}}"#,
        )
        .unwrap();
        fs::write(root.join("Makefile"), "serve:\n\tpython3 -m http.server\n").unwrap();
        fs::canonicalize(root).unwrap()
    }

    #[test]
    fn add_project_resolves_nested_folders_to_the_root_and_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = project_dir(tmp.path(), "Anılarım uygulaması");
        fs::create_dir_all(root.join("src/deep")).unwrap();
        let rt = runtime();
        let id = rt.add_project_path(&root.join("src/deep")).unwrap();
        assert_eq!(rt.add_project_path(&root).unwrap(), id);
        let stored = lock(&rt.db).get_project(id).unwrap();
        assert_eq!(Path::new(&stored.root_path), root);
        assert_eq!(lock(&rt.db).list_projects().unwrap().len(), 1);
    }

    #[test]
    fn add_project_accepts_plain_folders_but_never_dangerous_ones() {
        let tmp = TempDir::new().unwrap();
        let plain = tmp.path().join("notes folder");
        fs::create_dir_all(&plain).unwrap();
        let rt = runtime();
        let id = rt.add_project_path(&plain).unwrap();
        assert_eq!(
            Path::new(&lock(&rt.db).get_project(id).unwrap().root_path),
            fs::canonicalize(&plain).unwrap()
        );
        let file = tmp.path().join("a.txt");
        fs::write(&file, "x").unwrap();
        for bad in [
            Path::new("/"),
            Path::new("/usr/bin"),
            Path::new("/opt/homebrew"),
        ] {
            assert_eq!(
                rt.add_project_path(bad).unwrap_err().code(),
                crate::error::ErrorCode::InvalidInput,
                "{bad:?}"
            );
        }
        assert_eq!(
            rt.add_project_path(&file).unwrap_err().code(),
            crate::error::ErrorCode::PathNotFound
        );
        assert_eq!(
            rt.add_project_path(&tmp.path().join("missing"))
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::PathNotFound
        );
        if let Some(home) = std::env::var_os("HOME") {
            assert!(
                rt.add_project_path(Path::new(&home)).is_err(),
                "home itself is not a project"
            );
        }
    }

    #[test]
    fn export_import_round_trips_into_a_fresh_runtime() {
        let tmp = TempDir::new().unwrap();
        let root = project_dir(tmp.path(), "web app");
        let source = runtime();
        let id = source.add_project_path(&root).unwrap();
        source
            .set_project_override(
                id,
                ProjectOverrides {
                    display_name: Some("Pretty".into()),
                    primary_url: Some("localhost:4321".into()),
                    preferred_editor: None,
                    preferred_terminal: None,
                    auto_restart: true,
                },
            )
            .unwrap();
        source.set_favorite(id, true).unwrap();
        source
            .set_component_kind(id, "vite@.", Some(ComponentKind::Api))
            .unwrap();
        source
            .set_primary_command(id, Some("pkg:dev".into()))
            .unwrap();
        source
            .set_launch_profiles(
                id,
                vec![LaunchProfileInput {
                    id: None,
                    name: "Both".into(),
                    options: vec!["pkg:dev".into(), "make:serve".into()],
                }],
            )
            .unwrap();
        let mut settings = source.settings();
        settings.theme = Theme::Dark;
        settings.forget_after_days = Some(30);
        settings.launch_at_login = true;
        source.update_settings(settings).unwrap();

        let json = source.export_overrides_json().unwrap();
        assert!(
            !json.contains("\"program\"")
                && !json.contains("\"args\"")
                && !json.contains("\"cwd\""),
            "no raw commands in exports"
        );
        assert!(!json.contains("launchAtLogin"));

        let target = runtime();
        let report = target.import_overrides_json(&json).unwrap();
        assert_eq!(report.applied, 1);
        assert!(report.settings_applied);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        let new_id = *lock(&target.state)
            .by_path
            .get(&root.to_string_lossy().to_string())
            .unwrap();
        let record = lock(&target.db).get_project(new_id).unwrap();
        assert_eq!(record.display_name.as_deref(), Some("Pretty"));
        assert_eq!(record.primary_url.as_deref(), Some("http://localhost:4321"));
        assert!(record.auto_restart && record.favorite);
        assert_eq!(record.component_kinds["vite@."], ComponentKind::Api);
        assert_eq!(record.primary_command.unwrap().args, vec!["run", "dev"]);
        assert_eq!(record.launch_profiles[0].commands.len(), 2);
        assert_eq!(record.launch_profiles[0].commands[1].program, "make");
        let imported = target.settings();
        assert_eq!(imported.theme, Theme::Dark);
        assert_eq!(imported.forget_after_days, Some(30));
        assert!(!imported.launch_at_login, "never imported");
    }

    #[test]
    fn hostile_or_stale_files_cannot_inject_commands_or_break_validation() {
        let tmp = TempDir::new().unwrap();
        let root = project_dir(tmp.path(), "victim");
        let rt = runtime();
        let evil = serde_json::json!({
            "version": 1,
            "settings": { "launchAtLogin": true, "globalShortcut": "K", "theme": "dark" },
            "projects": [
                {
                    "rootPath": root,
                    "primaryOption": "pkg:dev",
                    "primaryUrl": "file:///etc/passwd",
                    "preferredEditor": "emacs-gui",
                    "preferredTerminal": "hyper",
                    "primaryCommand": { "program": "rm", "args": ["-rf", "/"], "cwd": "/" },
                    "program": "rm", "args": ["-rf", "/"],
                    "launchProfiles": [
                        { "name": "x", "options": ["bash"], "commands": [{ "program": "rm" }] }
                    ]
                },
                { "rootPath": "/definitely/not/here" },
                { "rootPath": "relative/path" },
                { "rootPath": "/usr/bin" }
            ]
        });
        let report = rt.import_overrides_json(&evil.to_string()).unwrap();
        assert_eq!(report.applied, 1);
        let reasons: Vec<String> = report
            .skipped
            .iter()
            .map(|s| format!("{}: {}", s.subject, s.reason))
            .collect();
        for needle in [
            "primary URL",
            "editor",
            "terminal",
            "launch profiles",
            "/definitely/not/here",
            "relative/path",
            "/usr/bin",
            "settings",
        ] {
            assert!(
                reasons.iter().any(|r| r.contains(needle)),
                "missing {needle:?} in {reasons:?}"
            );
        }
        let id = *lock(&rt.state)
            .by_path
            .get(&root.to_string_lossy().to_string())
            .unwrap();
        let record = lock(&rt.db).get_project(id).unwrap();
        assert_eq!(
            record.primary_command.as_ref().unwrap().program,
            "npm",
            "re-derived, not injected"
        );
        assert!(record.primary_url.is_none() && record.preferred_editor.is_none());
        assert!(record.launch_profiles.is_empty());
        assert!(!rt.settings().launch_at_login);
        assert_eq!(
            lock(&rt.db).list_projects().unwrap().len(),
            1,
            "skipped paths create nothing"
        );
    }

    #[test]
    fn malformed_oversized_and_future_files_are_rejected() {
        let rt = runtime();
        for bad in [
            "",
            "not json",
            "[]",
            r#"{"version":2}"#,
            r#"{"projects":[]}"#,
        ] {
            assert_eq!(
                rt.import_overrides_json(bad).unwrap_err().code(),
                crate::error::ErrorCode::InvalidInput,
                "{bad:?}"
            );
        }
        let huge = format!(
            r#"{{"version":1,"pad":"{}"}}"#,
            "x".repeat(MAX_IMPORT_BYTES)
        );
        assert_eq!(
            rt.import_overrides_json(&huge).unwrap_err().code(),
            crate::error::ErrorCode::InvalidInput
        );
        let empty = rt.import_overrides_json(r#"{"version":1}"#).unwrap();
        assert_eq!(
            (empty.applied, empty.settings_applied, empty.skipped.len()),
            (0, false, 0)
        );
    }
}
