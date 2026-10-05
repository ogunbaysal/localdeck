//! LocalDeck: a project-centric local development control center.
//!
//! React renders; Rust *is* LocalDeck. Layers: `commands` (Tauri IPC) →
//! `services` (orchestration) → `engine` (discovery) / `persistence`
//! (SQLite) / `platform` (OS actions).

pub mod commands;
pub mod engine;
pub mod error;
pub mod models;
pub mod notify;
pub mod persistence;
pub mod platform;
pub mod services;
pub mod shortcut;
pub mod tray;

use engine::Engine;
use models::Settings;
use persistence::Database;
use services::runtime::Runtime;
use services::scanner::{spawn_scanner, Scanner};
use std::sync::Arc;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Full `AppSnapshot` payload, emitted after every scan (~2 s).
pub const RUNTIME_EVENT: &str = "runtime://updated";
/// Project id payload, emitted when a tray project entry is chosen.
pub const OPEN_PROJECT_EVENT: &str = "ui://open-project";
/// Emitted by the global shortcut: show the window and open the command palette.
pub const OPEN_PALETTE_EVENT: &str = "ui://open-palette";
/// Emitted when a tray quick action fails (payload: `ErrorPayload`).
pub const ACTION_FAILED_EVENT: &str = "ui://action-failed";
const DATABASE_FILE: &str = "localdeck.db";

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,localdeck_lib=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// The OS is the source of truth for launch-at-login (the user can change it
/// outside the app); keep the stored setting honest.
fn reconcile_autostart(app: &tauri::AppHandle, runtime: &Runtime) {
    let stored = runtime.settings();
    if let Ok(actual) = app.autolaunch().is_enabled() {
        if actual != stored.launch_at_login {
            let corrected = Settings {
                launch_at_login: actual,
                ..stored
            };
            if let Err(err) = runtime.update_settings(corrected) {
                tracing::warn!(%err, "could not store launch-at-login state");
            }
        }
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let home = app.path().home_dir().ok();

    // A database that cannot be opened must not crash the app or pretend to
    // persist: fall back to memory and say so in the scanner status.
    let (db, notice) = match Database::open(&data_dir.join(DATABASE_FILE)) {
        Ok(db) => (db, None),
        Err(err) => {
            tracing::error!(%err, "database unavailable; running without persistence");
            (Database::open_in_memory()?, Some(err.payload()))
        }
    };
    let runtime = Arc::new(Runtime::new(db, Engine::new(home))?);
    if let Some(notice) = notice {
        runtime.set_persistence_notice(notice);
    }
    reconcile_autostart(&handle, &runtime);

    // First scan before the UI asks, so `get_snapshot` is never empty-but-wrong.
    let mut scanner = Scanner::new();
    let first = scanner.scan_once(&runtime);
    app.manage(runtime.clone());

    // Resolve the user's login-shell PATH off the scanner thread, so the first
    // Start click does not wait for it.
    std::thread::spawn(|| {
        platform::user_path();
    });

    // Restore the global shortcut; a failure (taken by another app) is not fatal.
    if let Some(accelerator) = runtime.settings().global_shortcut {
        if let Err(err) = shortcut::apply(&handle, None, Some(&accelerator)) {
            tracing::warn!(%err, "could not register the global shortcut");
        }
    }

    let tray_ok = tray::install(&handle, &first);
    // Startup detections are not alert-worthy.
    runtime.drain_notifications();
    let emitter = handle.clone();
    let alerts = runtime.clone();
    let pacing = runtime.clone();
    spawn_scanner(runtime, scanner, move |snapshot| {
        // Nobody renders the snapshot while the window is hidden; skip the
        // serialisation and IPC. The tray and notifications still update.
        if pacing.pacing.window_active() {
            if let Err(err) = emitter.emit(RUNTIME_EVENT, &*snapshot) {
                tracing::warn!(%err, "could not emit runtime update");
            }
        }
        tray::refresh(&emitter, &snapshot);
        notify::deliver(&emitter, &alerts.drain_notifications());
    })?;

    // Launched at login (`--hidden`) with a tray: stay out of the way.
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");
    if !(start_hidden && tray_ok) {
        tray::show_main_window(&handle);
    } else {
        // Tray-only start: begin on the slow background cadence.
        tray::sync_window_state(&handle);
    }
    Ok(())
}

/// The complete IPC surface. Every entry is a narrow, task-specific command.
pub fn register_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::get_snapshot,
        commands::get_settings,
        commands::update_settings,
        commands::list_editors,
        commands::list_terminals,
        commands::set_project_override,
        commands::set_component_kind,
        commands::set_project_favorite,
        commands::ignore_project,
        commands::forget_project,
        commands::forget_stopped_projects,
        commands::refresh_project,
        commands::list_project_scripts,
        commands::set_primary_command,
        commands::dismiss_attention,
        commands::start_project,
        commands::restart_project,
        commands::stop_project,
        commands::force_stop_project,
        commands::terminate_process,
        commands::force_kill_process,
        commands::open_project_url,
        commands::open_port_url,
        commands::open_project_folder,
        commands::open_project_editor,
        commands::open_project_terminal,
        commands::open_process_cwd,
        commands::set_launch_profiles,
        commands::suggest_free_port,
        commands::pick_and_add_project,
        commands::export_overrides,
        commands::import_overrides,
        commands::list_activity,
        commands::clear_activity,
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    let app = register_commands(tauri::Builder::default())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(shortcut::plugin())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                // Closing the window keeps LocalDeck scanning from the tray.
                WindowEvent::CloseRequested { api, .. } => {
                    if tray::is_available(window.app_handle()) {
                        api.prevent_close();
                        let _ = window.hide();
                        tray::sync_window_state(window.app_handle());
                    }
                }
                WindowEvent::Focused(_) | WindowEvent::Resized(_) => {
                    tray::sync_window_state(window.app_handle());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building LocalDeck");

    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            tray::show_main_window(handle);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}
