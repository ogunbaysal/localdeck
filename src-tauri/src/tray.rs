//! Native tray / menu-bar item: a glance surface with quick actions, not a
//! second UI. The menu is described by a pure [`MenuEntry`] model (tested
//! without native menus) and rebuilt only when that model changes. Failure to
//! create a tray (some Linux desktops) is non-fatal.

use crate::error::{AppError, ErrorPayload};
use crate::models::{AppSnapshot, Project, ProjectId, RuntimeStatus};
use crate::services::runtime::Runtime;
use crate::{ACTION_FAILED_EVENT, OPEN_PROJECT_EVENT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager};

const MAX_TRAY_PROJECTS: usize = 8;
const MAX_TRAY_PORTS: usize = 3;
const MAX_START_ENTRIES: usize = 8;
const TRAY_ID: &str = "localdeck-tray";
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";
const ID_PROJECT_PREFIX: &str = "project:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenUrl,
    Restart,
    Stop,
    Details,
    Start,
}

impl TrayAction {
    fn slug(self) -> &'static str {
        match self {
            Self::OpenUrl => "open",
            Self::Restart => "restart",
            Self::Stop => "stop",
            Self::Details => "details",
            Self::Start => "start",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Some(match slug {
            "open" => Self::OpenUrl,
            "restart" => Self::Restart,
            "stop" => Self::Stop,
            "details" => Self::Details,
            "start" => Self::Start,
            _ => return None,
        })
    }
}

/// `project:<id>:<action>`.
pub fn action_id(project: ProjectId, action: TrayAction) -> String {
    format!("{ID_PROJECT_PREFIX}{project}:{}", action.slug())
}

/// Inverse of [`action_id`]. A bare `project:<id>` means "details".
pub fn parse_action_id(id: &str) -> Option<(ProjectId, TrayAction)> {
    let rest = id.strip_prefix(ID_PROJECT_PREFIX)?;
    match rest.split_once(':') {
        Some((project, action)) => Some((
            project.parse::<ProjectId>().ok()?,
            TrayAction::from_slug(action)?,
        )),
        None => Some((rest.parse::<ProjectId>().ok()?, TrayAction::Details)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

fn item(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> MenuEntry {
    MenuEntry::Item {
        id: id.into(),
        label: label.into(),
        enabled,
    }
}

fn running_label(project: &Project) -> String {
    let marker = if project.runtime_status == RuntimeStatus::Partial {
        "◐"
    } else {
        "●"
    };
    let mut label = format!("{marker} {}", project.name);
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
    label
}

/// The whole tray menu for a snapshot.
pub fn build_model(snapshot: &AppSnapshot) -> Vec<MenuEntry> {
    let visible = snapshot.projects.iter().filter(|p| !p.ignored);
    let mut active: Vec<&Project> = visible
        .clone()
        .filter(|p| p.runtime_status != RuntimeStatus::Stopped)
        .collect();
    active.sort_by_key(|p| p.name.to_lowercase());
    let mut startable: Vec<&Project> = visible
        .filter(|p| p.runtime_status == RuntimeStatus::Stopped && p.primary_command.is_some())
        .collect();
    // Favourites first, then by name.
    startable.sort_by_key(|p| (!p.favorite, p.name.to_lowercase()));

    let mut menu = vec![item("title", "LocalDeck", false), MenuEntry::Separator];
    if active.is_empty() {
        menu.push(item("none", "No running projects", false));
    }
    for project in active.iter().take(MAX_TRAY_PROJECTS) {
        let mut entries = Vec::new();
        if project.primary_url.is_some() {
            entries.push(item(
                action_id(project.id, TrayAction::OpenUrl),
                "Open URL",
                true,
            ));
        }
        entries.push(item(
            action_id(project.id, TrayAction::Restart),
            "Restart",
            project.primary_command.is_some(),
        ));
        entries.push(item(action_id(project.id, TrayAction::Stop), "Stop", true));
        entries.push(MenuEntry::Separator);
        entries.push(item(
            action_id(project.id, TrayAction::Details),
            "Details",
            true,
        ));
        menu.push(MenuEntry::Submenu {
            label: running_label(project),
            entries,
        });
    }
    if active.len() > MAX_TRAY_PROJECTS {
        menu.push(item(
            "more",
            format!("+{} more", active.len() - MAX_TRAY_PROJECTS),
            false,
        ));
    }
    if !startable.is_empty() {
        menu.push(MenuEntry::Separator);
        menu.push(MenuEntry::Submenu {
            label: "Start".to_string(),
            entries: startable
                .iter()
                .take(MAX_START_ENTRIES)
                .map(|p| {
                    let label = if p.favorite {
                        format!("★ {}", p.name)
                    } else {
                        p.name.clone()
                    };
                    item(action_id(p.id, TrayAction::Start), label, true)
                })
                .collect(),
        });
    }
    menu.push(MenuEntry::Separator);
    menu.push(item(ID_OPEN, "Open LocalDeck", true));
    menu.push(item(ID_QUIT, "Quit LocalDeck", true));
    menu
}

/// Number of running (or partial) projects, for the macOS menu-bar title.
pub fn running_count(snapshot: &AppSnapshot) -> usize {
    snapshot
        .projects
        .iter()
        .filter(|p| !p.ignored && p.runtime_status != RuntimeStatus::Stopped)
        .count()
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
    sync_window_state(app);
}

/// Re-read whether the window is visible (and not minimized) and tell the
/// scanner. When it becomes visible the UI gets the cached snapshot right away
/// plus a fresh scan, instead of waiting for the slow background cadence.
pub fn sync_window_state(app: &AppHandle) {
    let Some(runtime) = app.try_state::<Arc<Runtime>>() else {
        return;
    };
    let active = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    let was = runtime.pacing.window_active();
    runtime.pacing.set_window_active(active);
    if active && !was {
        let _ = app.emit(crate::RUNTIME_EVENT, &*runtime.snapshot());
        runtime.request_scan();
    }
}

/// Hovering or clicking the tray icon: the menu is probably about to open, so
/// scan now and keep a medium cadence for a while.
fn tray_interacted(app: &AppHandle) {
    let Some(runtime) = app.try_state::<Arc<Runtime>>() else {
        return;
    };
    let first =
        runtime.pacing.mode(std::time::Instant::now()) == crate::services::pacing::Mode::Background;
    runtime.pacing.tray_interacted(std::time::Instant::now());
    if first {
        runtime.request_scan();
    }
}

fn append_entries(
    app: &AppHandle,
    entries: &[MenuEntry],
    add: &mut dyn FnMut(&dyn tauri::menu::IsMenuItem<tauri::Wry>) -> tauri::Result<()>,
) -> tauri::Result<()> {
    for entry in entries {
        match entry {
            MenuEntry::Separator => add(&PredefinedMenuItem::separator(app)?)?,
            MenuEntry::Item { id, label, enabled } => {
                add(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?
            }
            MenuEntry::Submenu { label, entries } => {
                let submenu = Submenu::new(app, label, true)?;
                append_entries(app, entries, &mut |item| submenu.append(item))?;
                add(&submenu)?;
            }
        }
    }
    Ok(())
}

fn build_menu(app: &AppHandle, model: &[MenuEntry]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    append_entries(app, model, &mut |item| menu.append(item))?;
    Ok(menu)
}

/// Run a tray quick action off the main thread; failures are reported to the
/// UI (`ui://action-failed`) and as a native notification.
fn run_action(app: &AppHandle, project: ProjectId, action: TrayAction) {
    if action == TrayAction::Details {
        show_main_window(app);
        let _ = app.emit(OPEN_PROJECT_EVENT, project);
        return;
    }
    let Some(runtime) = app.try_state::<Arc<Runtime>>().map(|s| (*s).clone()) else {
        return;
    };
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = match action {
            TrayAction::OpenUrl => runtime.open_project_url(project),
            TrayAction::Restart => runtime.restart_project(project, None, None),
            TrayAction::Stop => runtime.stop_project(project, false).map(|_| ()),
            TrayAction::Start => runtime.start_project(project, None, None),
            TrayAction::Details => Ok(()),
        };
        if let Err(err) = result {
            report_failure(&handle, &runtime, project, &err);
        }
    });
}

fn report_failure(app: &AppHandle, runtime: &Runtime, project: ProjectId, error: &AppError) {
    let payload: ErrorPayload = error.payload();
    let _ = app.emit(ACTION_FAILED_EVENT, &payload);
    let title = runtime
        .find_project(project)
        .map(|p| p.name)
        .unwrap_or_else(|_| "LocalDeck".to_string());
    crate::notify::show(app, &title, &payload.message);
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_OPEN => show_main_window(app),
        ID_QUIT => app.exit(0),
        other => {
            if let Some((project, action)) = parse_action_id(other) {
                run_action(app, project, action);
            }
        }
    }
}

fn model_key(model: &[MenuEntry]) -> String {
    format!("{model:?}")
}

/// Creates the tray icon. Returns whether it exists afterwards.
pub fn install(app: &AppHandle, snapshot: &AppSnapshot) -> bool {
    let state = TrayState::default();
    let model = build_model(snapshot);
    let built = (|| -> tauri::Result<TrayIcon> {
        let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        let menu = build_menu(app, &model)?;
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .tooltip("LocalDeck")
            .menu(&menu)
            .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
            .on_tray_icon_event(|tray, event| {
                use tauri::tray::TrayIconEvent;
                if matches!(
                    event,
                    TrayIconEvent::Enter { .. } | TrayIconEvent::Click { .. }
                ) {
                    tray_interacted(tray.app_handle());
                }
            })
            .build(app)
    })();
    match built {
        Ok(icon) => {
            *state.icon.lock().unwrap_or_else(|p| p.into_inner()) = Some(icon);
            *state.last_key.lock().unwrap_or_else(|p| p.into_inner()) = model_key(&model);
            state.available.store(true, Ordering::Relaxed);
        }
        Err(err) => {
            tracing::warn!(%err, "tray icon unavailable; the main window stays the only surface")
        }
    }
    let available = state.available.load(Ordering::Relaxed);
    app.manage(state);
    if available {
        update_title(app, running_count(snapshot));
    }
    available
}

fn update_title(app: &AppHandle, running: usize) {
    #[cfg(target_os = "macos")]
    if let Some(state) = app.try_state::<TrayState>() {
        if let Some(icon) = state
            .icon
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            let title = (running > 0).then(|| running.to_string());
            let _ = icon.set_title(title);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, running);
}

/// Rebuild the menu when (and only when) the model changed.
pub fn refresh(app: &AppHandle, snapshot: &AppSnapshot) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if !state.available.load(Ordering::Relaxed) {
        return;
    }
    let model = build_model(snapshot);
    let running = running_count(snapshot);
    {
        let mut last = state.last_key.lock().unwrap_or_else(|p| p.into_inner());
        let key = model_key(&model);
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
        match build_menu(&handle, &model) {
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
        update_title(&handle, running);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AppSnapshot, Metrics, Platform, PrimaryCommand, PrimaryUrl, PrimaryUrlSource, RootKind,
        ScannerStatus,
    };

    fn project(id: i64, name: &str, status: RuntimeStatus) -> Project {
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
            ports: vec![],
            primary_url: None,
            primary_command: None,
            launch_profiles: vec![],
            overrides: Default::default(),
            metrics: Metrics::default(),
            first_seen_at: 0,
            last_seen_at: 0,
            favorite: false,
            ignored: false,
            recent_activity: vec![],
        }
    }

    fn command() -> PrimaryCommand {
        PrimaryCommand {
            program: "bun".into(),
            args: vec!["run".into(), "dev".into()],
            cwd: "/work".into(),
            label: "bun run dev".into(),
            script: Some("pkg:dev".into()),
        }
    }

    fn url() -> PrimaryUrl {
        PrimaryUrl {
            url: "http://localhost:3000".into(),
            source: PrimaryUrlSource::WebComponent,
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

    fn submenu<'a>(menu: &'a [MenuEntry], prefix: &str) -> Option<&'a Vec<MenuEntry>> {
        menu.iter().find_map(|e| match e {
            MenuEntry::Submenu { label, entries } if label.contains(prefix) => Some(entries),
            _ => None,
        })
    }

    fn ids(entries: &[MenuEntry]) -> Vec<(String, String, bool)> {
        entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item { id, label, enabled } => {
                    Some((id.clone(), label.clone(), *enabled))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn action_ids_round_trip_and_reject_garbage() {
        for action in [
            TrayAction::OpenUrl,
            TrayAction::Restart,
            TrayAction::Stop,
            TrayAction::Details,
            TrayAction::Start,
        ] {
            assert_eq!(parse_action_id(&action_id(42, action)), Some((42, action)));
        }
        assert_eq!(parse_action_id("project:7"), Some((7, TrayAction::Details)));
        for bad in [
            "open",
            "project:",
            "project:x:stop",
            "project:1:format-disk",
            "project:1:stop:extra",
            "",
        ] {
            assert_eq!(parse_action_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn running_projects_get_quick_actions_only_where_they_apply() {
        let mut web = project(1, "Zeta", RuntimeStatus::Running);
        web.ports = vec![3000, 4000, 5000, 6000];
        web.primary_url = Some(url());
        web.primary_command = Some(command());
        let bare = project(2, "alpha", RuntimeStatus::Partial);
        let mut hidden = project(3, "Hidden", RuntimeStatus::Running);
        hidden.ignored = true;
        let model = build_model(&snapshot(vec![web, bare, hidden]));
        let zeta = submenu(&model, "Zeta").unwrap();
        let actions = ids(zeta);
        assert_eq!(
            actions,
            vec![
                ("project:1:open".to_string(), "Open URL".to_string(), true),
                ("project:1:restart".to_string(), "Restart".to_string(), true),
                ("project:1:stop".to_string(), "Stop".to_string(), true),
                ("project:1:details".to_string(), "Details".to_string(), true),
            ]
        );
        let labels: Vec<&str> = model
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Submenu { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels[0], "◐ alpha",
            "sorted case-insensitively, partial marker"
        );
        assert!(labels[1].starts_with("● Zeta    :3000 :4000 :5000…"));
        let alpha = submenu(&model, "alpha").unwrap();
        let alpha = ids(alpha);
        assert_eq!(alpha.len(), 3, "no URL → no Open URL");
        assert_eq!(
            alpha[0],
            ("project:2:restart".into(), "Restart".into(), false),
            "no command → restart disabled"
        );
    }

    #[test]
    fn stopped_projects_with_a_command_appear_under_start_favourites_first_and_capped() {
        let mut projects: Vec<Project> = (0..12)
            .map(|i| {
                let mut p = project(i, &format!("p{i:02}"), RuntimeStatus::Stopped);
                p.primary_command = Some(command());
                p
            })
            .collect();
        projects[9].favorite = true;
        projects.push(project(99, "no-command", RuntimeStatus::Stopped));
        let mut ignored = project(98, "ignored", RuntimeStatus::Stopped);
        ignored.primary_command = Some(command());
        ignored.ignored = true;
        projects.push(ignored);
        let model = build_model(&snapshot(projects));
        let start = ids(submenu(&model, "Start").unwrap());
        assert_eq!(start.len(), MAX_START_ENTRIES);
        assert_eq!(start[0], ("project:9:start".into(), "★ p09".into(), true));
        assert_eq!(start[1].1, "p00");
        assert!(start
            .iter()
            .all(|(id, _, _)| id != "project:99:start" && id != "project:98:start"));
    }

    #[test]
    fn empty_and_overflowing_states() {
        let none = build_model(&snapshot(vec![project(1, "a", RuntimeStatus::Stopped)]));
        assert!(none.contains(&item("none", "No running projects", false)));
        assert!(
            submenu(&none, "Start").is_none(),
            "nothing startable → no Start menu"
        );
        assert!(none.contains(&item(ID_OPEN, "Open LocalDeck", true)));
        assert!(none.contains(&item(ID_QUIT, "Quit LocalDeck", true)));

        let many: Vec<Project> = (0..11)
            .map(|i| project(i, &format!("r{i:02}"), RuntimeStatus::Running))
            .collect();
        let model = build_model(&snapshot(many));
        assert_eq!(
            model
                .iter()
                .filter(|e| matches!(e, MenuEntry::Submenu { .. }))
                .count(),
            MAX_TRAY_PROJECTS
        );
        assert!(model.contains(&item("more", "+3 more", false)));
    }

    #[test]
    fn the_model_key_changes_with_content_only() {
        let a = build_model(&snapshot(vec![project(1, "a", RuntimeStatus::Running)]));
        let same = build_model(&snapshot(vec![project(1, "a", RuntimeStatus::Running)]));
        let different = build_model(&snapshot(vec![project(1, "a", RuntimeStatus::Stopped)]));
        assert_eq!(model_key(&a), model_key(&same));
        assert_ne!(model_key(&a), model_key(&different));
        let mut busy = project(1, "a", RuntimeStatus::Running);
        busy.metrics.cpu_percent = 99.0; // metrics never rebuild the menu
        assert_eq!(
            model_key(&a),
            model_key(&build_model(&snapshot(vec![busy])))
        );
    }
}
