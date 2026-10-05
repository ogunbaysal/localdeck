use super::blocking;
use crate::error::{AppError, AppResult};
use crate::models::{EditorInfo, Settings, TerminalInfo};
use crate::platform;
use crate::services::runtime::Runtime;
use std::sync::Arc;
use tauri::{AppHandle, Runtime as TauriRuntime, State};
use tauri_plugin_autostart::ManagerExt;

type Rt<'a> = State<'a, Arc<Runtime>>;

#[tauri::command]
pub async fn get_settings(runtime: Rt<'_>) -> AppResult<Settings> {
    Ok(runtime.settings())
}

/// Replaces the settings. OS-level effects (launch at login, global shortcut)
/// are applied first so the stored value never claims something the OS
/// refused; if saving then fails they are rolled back.
#[tauri::command]
pub async fn update_settings<R: TauriRuntime>(
    app: AppHandle<R>,
    runtime: Rt<'_>,
    settings: Settings,
) -> AppResult<Settings> {
    let previous = runtime.settings();
    let mut wanted = settings;
    wanted.global_shortcut = match wanted.global_shortcut.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(crate::shortcut::validate_accelerator(raw)?),
    };
    let autostart_changed = wanted.launch_at_login != previous.launch_at_login;
    crate::shortcut::apply(
        &app,
        previous.global_shortcut.as_deref(),
        wanted.global_shortcut.as_deref(),
    )?;
    if autostart_changed {
        if let Err(err) = set_autostart(&app, wanted.launch_at_login) {
            let _ = crate::shortcut::apply(
                &app,
                wanted.global_shortcut.as_deref(),
                previous.global_shortcut.as_deref(),
            );
            return Err(err);
        }
    }
    let to_save = wanted.clone();
    match blocking(&runtime, move |rt| rt.update_settings(to_save)).await {
        Ok(saved) => Ok(saved),
        Err(err) => {
            if autostart_changed {
                let _ = set_autostart(&app, previous.launch_at_login);
            }
            let _ = crate::shortcut::apply(
                &app,
                wanted.global_shortcut.as_deref(),
                previous.global_shortcut.as_deref(),
            );
            Err(err)
        }
    }
}

fn set_autostart<R: TauriRuntime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|err| AppError::Unsupported(format!("Could not change launch at login: {err}")))
}

#[tauri::command]
pub async fn list_editors(runtime: Rt<'_>) -> AppResult<Vec<EditorInfo>> {
    blocking(&runtime, |_| Ok(platform::detect_editors())).await
}

#[tauri::command]
pub async fn list_terminals(runtime: Rt<'_>) -> AppResult<Vec<TerminalInfo>> {
    blocking(&runtime, |_| Ok(platform::detect_terminals())).await
}
