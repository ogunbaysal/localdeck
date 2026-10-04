//! Native tray / menu-bar item: a glance surface (running projects, open,
//! quit), not a second UI. Failure to create a tray (some Linux desktops) is
//! non-fatal: the main window then behaves like a normal window.

use crate::models::{AppSnapshot, ProjectId, RuntimeStatus};
use crate::OPEN_PROJECT_EVENT;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager};

const MAX_TRAY_PROJECTS: usize = 8;
const MAX_TRAY_PORTS: usize = 3;
const TRAY_ID: &str = "localdeck-tray";
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";
const ID_PROJECT_PREFIX: &str = "project:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLine {
    pub project_id: ProjectId,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraySummary {
    pub lines: Vec<TrayLine>,
    pub hidden_count: usize,
    pub running: usize,
}

impl TraySummary {
    /// Cheap equality key so the native menu is only rebuilt when it changes.
    fn key(&self) -> String {
        let mut key = format!("{}|{}", self.running, self.hidden_count);
        for line in &self.lines {
            key.push('|');
            key.push_str(&line.project_id.to_string());
            key.push(':');
            key.push_str(&line.label);
        }
        key
    }
}

/// Running (and partial) non-ignored projects with their main ports.
pub fn summarize(snapshot: &AppSnapshot) -> TraySummary {
    let mut active: Vec<&crate::models::Project> = snapshot
        .projects
        .iter()
        .filter(|p| !p.ignored && p.runtime_status != RuntimeStatus::Stopped)
        .collect();
    active.sort_by_key(|p| p.name.to_lowercase());
    let running = active.len();
    let lines = active
        .iter()
        .take(MAX_TRAY_PROJECTS)
        .map(|project| {
            let mut label = format!("● {}", project.name);
            if project.runtime_status == RuntimeStatus::Partial {
                label = format!("◐ {}", project.name);
            }
            let ports: Vec<String> = project
                .ports
                .iter()
                .take(MAX_TRAY_PORTS)
                .map(|p| format!(":{p}"))
                .collect();
            if !ports.is_empty() {
                label.push_str("    ");
                label.push_str(&ports.join(" "));
                if project.ports.len() > MAX_TRAY_PORTS {
                    label.push('…');
                }
            }
            TrayLine {
                project_id: project.id,
                label,
            }
        })
        .collect();
    TraySummary {
        lines,
        hidden_count: running.saturating_sub(MAX_TRAY_PROJECTS),
        running,
    }
}

/// Whether a tray icon exists; decides if closing the window hides it.
#[derive(Default)]
pub struct TrayState {
    available: AtomicBool,
    icon: Mutex<Option<TrayIcon>>,
    last_key: Mutex<String>,
}

pub fn is_available(app: &AppHandle) -> bool {
    app.try_state::<TrayState>()
        .is_some_and(|state| state.available.load(Ordering::Relaxed))
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_menu(app: &AppHandle, summary: &TraySummary) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "title",
        "LocalDeck",
        false,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if summary.lines.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "none",
            "No running projects",
            false,
            None::<&str>,
        )?)?;
    }
    for line in &summary.lines {
        menu.append(&MenuItem::with_id(
            app,
            format!("{ID_PROJECT_PREFIX}{}", line.project_id),
            &line.label,
            true,
            None::<&str>,
        )?)?;
    }
    if summary.hidden_count > 0 {
        menu.append(&MenuItem::with_id(
            app,
            "more",
            format!("+{} more", summary.hidden_count),
            false,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        ID_OPEN,
        "Open LocalDeck",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        ID_QUIT,
        "Quit LocalDeck",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_OPEN => show_main_window(app),
        ID_QUIT => app.exit(0),
        other => {
            if let Some(project) = other
                .strip_prefix(ID_PROJECT_PREFIX)
                .and_then(|raw| raw.parse::<ProjectId>().ok())
            {
                show_main_window(app);
                let _ = app.emit(OPEN_PROJECT_EVENT, project);
            }
        }
    }
}

/// Creates the tray icon. Returns whether it exists afterwards.
pub fn install(app: &AppHandle, summary: &TraySummary) -> bool {
    let state = TrayState::default();
    let built = (|| -> tauri::Result<TrayIcon> {
        let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        let menu = build_menu(app, summary)?;
        let builder = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .tooltip("LocalDeck")
            .menu(&menu)
            .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()));
        builder.build(app)
    })();
    match built {
        Ok(icon) => {
            *state.icon.lock().unwrap_or_else(|p| p.into_inner()) = Some(icon);
            *state.last_key.lock().unwrap_or_else(|p| p.into_inner()) = summary.key();
            state.available.store(true, Ordering::Relaxed);
        }
        Err(err) => {
            tracing::warn!(%err, "tray icon unavailable; the main window stays the only surface")
        }
    }
    let available = state.available.load(Ordering::Relaxed);
    app.manage(state);
    if available {
        update_title(app, summary);
    }
    available
}

fn update_title(app: &AppHandle, summary: &TraySummary) {
    #[cfg(target_os = "macos")]
    if let Some(state) = app.try_state::<TrayState>() {
        if let Some(icon) = state
            .icon
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            let title = (summary.running > 0).then(|| summary.running.to_string());
            let _ = icon.set_title(title);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, summary);
}

/// Rebuild the menu when (and only when) the running-project summary changed.
pub fn refresh(app: &AppHandle, snapshot: &AppSnapshot) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if !state.available.load(Ordering::Relaxed) {
        return;
    }
    let summary = summarize(snapshot);
    {
        let mut last = state.last_key.lock().unwrap_or_else(|p| p.into_inner());
        let key = summary.key();
        if *last == key {
            return;
        }
        *last = key;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = handle.try_state::<TrayState>() else {
            return;
        };
        match build_menu(&handle, &summary) {
            Ok(menu) => {
                if let Some(icon) = state
                    .icon
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                {
                    let _ = icon.set_menu(Some(menu));
                }
            }
            Err(err) => tracing::warn!(%err, "could not rebuild tray menu"),
        }
        update_title(&handle, &summary);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AppSnapshot, Metrics, Platform, Project, RootKind, ScannerStatus};

    fn project(
        id: i64,
        name: &str,
        status: RuntimeStatus,
        ports: &[u16],
        ignored: bool,
    ) -> Project {
        Project {
            id,
            name: name.to_string(),
            detected_name: name.to_string(),
            root_path: format!("/work/{name}"),
            root_kind: RootKind::Git,
            runtime_status: status,
            stop_pending: false,
            attention: vec![],
            stack: vec![],
            package_manager: None,
            components: vec![],
            processes: vec![],
            ports: ports.to_vec(),
            primary_url: None,
            primary_command: None,
            overrides: Default::default(),
            metrics: Metrics::default(),
            first_seen_at: 0,
            last_seen_at: 0,
            favorite: false,
            ignored,
            recent_activity: vec![],
        }
    }

    fn snapshot(projects: Vec<Project>) -> AppSnapshot {
        AppSnapshot {
            projects,
            ports: vec![],
            scanner: ScannerStatus {
                interval_ms: 2000,
                scan_count: 1,
                last_scan_ms: 1,
                process_access: Default::default(),
                sockets_available: true,
                socket_error: None,
                sockets_skipped: 0,
                persistence_error: None,
                platform: Platform::Macos,
            },
            updated_at: 0,
        }
    }

    #[test]
    fn summarises_only_active_visible_projects_with_ports() {
        let s = summarize(&snapshot(vec![
            project(1, "Zeta", RuntimeStatus::Running, &[3000, 4000], false),
            project(2, "alpha", RuntimeStatus::Partial, &[], false),
            project(3, "Stopped", RuntimeStatus::Stopped, &[1], false),
            project(4, "Hidden", RuntimeStatus::Running, &[2], true),
        ]));
        assert_eq!(s.running, 2);
        let labels: Vec<&str> = s.lines.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(labels, vec!["◐ alpha", "● Zeta    :3000 :4000"]);
        assert_eq!(s.lines[1].project_id, 1);
    }

    #[test]
    fn caps_the_list_and_truncates_long_port_lists() {
        let many: Vec<Project> = (0..12)
            .map(|i| {
                project(
                    i,
                    &format!("p{i:02}"),
                    RuntimeStatus::Running,
                    &[1, 2, 3, 4],
                    false,
                )
            })
            .collect();
        let s = summarize(&snapshot(many));
        assert_eq!(s.lines.len(), MAX_TRAY_PROJECTS);
        assert_eq!(s.hidden_count, 4);
        assert!(s.lines[0].label.ends_with(":1 :2 :3…"));
    }

    #[test]
    fn empty_summary_when_nothing_runs_and_key_tracks_changes() {
        let none = summarize(&snapshot(vec![project(
            1,
            "a",
            RuntimeStatus::Stopped,
            &[],
            false,
        )]));
        assert_eq!(none, TraySummary::default());
        let one = summarize(&snapshot(vec![project(
            1,
            "a",
            RuntimeStatus::Running,
            &[80],
            false,
        )]));
        assert_ne!(none.key(), one.key());
        let same = summarize(&snapshot(vec![project(
            1,
            "a",
            RuntimeStatus::Running,
            &[80],
            false,
        )]));
        assert_eq!(one.key(), same.key());
    }
}
