use crate::error::{AppError, AppResult};
use crate::models::{ImportReport, ImportSkip, ProjectId};
use crate::services::runtime::Runtime;
use crate::services::transfer::MAX_IMPORT_BYTES;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Runtime as TauriRuntime, State};
use tauri_plugin_dialog::DialogExt;

type Rt<'a> = State<'a, Arc<Runtime>>;

fn internal(err: impl std::fmt::Display) -> AppError {
    AppError::Internal(err.to_string())
}

/// Native folder picker; the chosen folder is remembered as a project.
#[tauri::command]
pub async fn pick_and_add_project<R: TauriRuntime>(
    app: AppHandle<R>,
    runtime: Rt<'_>,
) -> AppResult<Option<ProjectId>> {
    let runtime = (*runtime).clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(internal)?;
        runtime.add_project_path(&path).map(Some)
    })
    .await
    .map_err(internal)?
}

/// Writes `contents` next to `target` and renames it into place.
fn write_atomically(target: &Path, contents: &str) -> AppResult<()> {
    let dir = target
        .parent()
        .ok_or_else(|| AppError::PathNotFound(target.display().to_string()))?;
    let temp: PathBuf = dir.join(format!(".localdeck-export-{}.tmp", std::process::id()));
    std::fs::write(&temp, contents).map_err(internal)?;
    std::fs::rename(&temp, target).map_err(|err| {
        let _ = std::fs::remove_file(&temp);
        internal(err)
    })
}

/// Native save dialog; resolves to the written path (`None` if cancelled).
#[tauri::command]
pub async fn export_overrides<R: TauriRuntime>(
    app: AppHandle<R>,
    runtime: Rt<'_>,
) -> AppResult<Option<String>> {
    let runtime = (*runtime).clone();
    tauri::async_runtime::spawn_blocking(move || {
        let json = runtime.export_overrides_json()?;
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name("localdeck-overrides.json")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(internal)?;
        write_atomically(&path, &json)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(internal)?
}

/// Native open dialog; resolves to the import report (`None` if cancelled).
#[tauri::command]
pub async fn import_overrides<R: TauriRuntime>(
    app: AppHandle<R>,
    runtime: Rt<'_>,
) -> AppResult<Option<ImportReport>> {
    let runtime = (*runtime).clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(internal)?;
        let size = std::fs::metadata(&path).map_err(internal)?.len();
        if size > MAX_IMPORT_BYTES as u64 {
            return Err(AppError::InvalidInput(
                "The file is too large to be an overrides export".to_string(),
            ));
        }
        let json = std::fs::read_to_string(&path)
            .map_err(|e| AppError::InvalidInput(format!("Could not read the file: {e}")))?;
        let before = runtime.settings().global_shortcut;
        let mut report = runtime.import_overrides_json(&json)?;
        // The OS registration of an imported shortcut happens here; if it
        // cannot be registered the previous shortcut is restored.
        let after = runtime.settings().global_shortcut;
        if before != after {
            if let Err(err) = crate::shortcut::apply(&app, before.as_deref(), after.as_deref()) {
                let mut settings = runtime.settings();
                settings.global_shortcut = before;
                let _ = runtime.update_settings(settings);
                report.skipped.push(ImportSkip {
                    subject: "settings".to_string(),
                    reason: err.to_string(),
                });
            }
        }
        Ok(Some(report))
    })
    .await
    .map_err(internal)?
}
