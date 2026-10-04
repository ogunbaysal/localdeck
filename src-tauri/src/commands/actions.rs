use super::blocking;
use crate::error::AppResult;
use crate::models::ProjectId;
use crate::services::runtime::Runtime;
use std::sync::Arc;
use tauri::State;

type Rt<'a> = State<'a, Arc<Runtime>>;

#[tauri::command]
pub async fn open_project_url(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_project_url(project_id)).await
}

#[tauri::command]
pub async fn open_port_url(runtime: Rt<'_>, port_id: String) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_port_url(&port_id)).await
}

#[tauri::command]
pub async fn open_project_folder(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_project_folder(project_id)).await
}

#[tauri::command]
pub async fn open_project_editor(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_project_editor(project_id)).await
}

#[tauri::command]
pub async fn open_project_terminal(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_project_terminal(project_id)).await
}

#[tauri::command]
pub async fn open_process_cwd(runtime: Rt<'_>, pid: u32) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.open_process_cwd(pid)).await
}
