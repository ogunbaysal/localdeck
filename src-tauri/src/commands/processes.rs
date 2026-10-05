use super::blocking;
use crate::error::AppResult;
use crate::models::{ProjectId, StopReport};
use crate::services::runtime::Runtime;
use std::sync::Arc;
use tauri::State;

type Rt<'a> = State<'a, Arc<Runtime>>;

/// Graceful (SIGTERM) termination of one process shown in the UI.
#[tauri::command]
pub async fn terminate_process(
    runtime: Rt<'_>,
    pid: u32,
    started_at: Option<i64>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.signal_process(pid, started_at, false)
    })
    .await
}

/// Explicit force kill (SIGKILL); never invoked implicitly by any other command.
#[tauri::command]
pub async fn force_kill_process(
    runtime: Rt<'_>,
    pid: u32,
    started_at: Option<i64>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.signal_process(pid, started_at, true)).await
}

#[tauri::command]
pub async fn start_project(
    runtime: Rt<'_>,
    project_id: ProjectId,
    profile_id: Option<String>,
    port: Option<u16>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.start_project(project_id, profile_id, port)
    })
    .await
}

#[tauri::command]
pub async fn restart_project(
    runtime: Rt<'_>,
    project_id: ProjectId,
    profile_id: Option<String>,
    port: Option<u16>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.restart_project(project_id, profile_id, port)
    })
    .await
}

#[tauri::command]
pub async fn stop_project(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<StopReport> {
    blocking(&runtime, move |rt| rt.stop_project(project_id, false)).await
}

#[tauri::command]
pub async fn force_stop_project(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<StopReport> {
    blocking(&runtime, move |rt| rt.stop_project(project_id, true)).await
}

/// Next free loopback port at or after `preferred`.
#[tauri::command]
pub async fn suggest_free_port(runtime: Rt<'_>, preferred: u16) -> AppResult<u16> {
    blocking(&runtime, move |rt| rt.suggest_free_port(preferred)).await
}
