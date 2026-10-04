use super::blocking;
use crate::error::{AppError, AppResult};
use crate::models::{EditorInfo, Settings};
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

/// Replaces the settings. Launch-at-login is applied to the OS first so the
/// stored value never claims something the OS refused.
#[tauri::command]
pub async fn update_settings<R: TauriRuntime>(
    app: AppHandle<R>,
    runtime: Rt<'_>,
    settings: Settings,
) -> AppResult<Settings> {
    let previous = runtime.settings();
    if settings.launch_at_login != previous.launch_at_login {
        set_autostart(&app, settings.launch_at_login)?;
    }
    let wanted = settings.clone();
    match blocking(&runtime, move |rt| rt.update_settings(wanted)).await {
        Ok(saved) => Ok(saved),
        Err(err) => {
            if settings.launch_at_login != previous.launch_at_login {
                let _ = set_autostart(&app, previous.launch_at_login);
            }
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
