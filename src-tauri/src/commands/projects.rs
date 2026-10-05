use super::blocking;
use crate::error::AppResult;
use crate::models::{
    AppSnapshot, ComponentKind, LaunchProfileInput, ProjectId, ProjectOverrides, ProjectScripts,
};
use crate::services::runtime::Runtime;
use std::sync::Arc;
use tauri::State;

type Rt<'a> = State<'a, Arc<Runtime>>;

#[tauri::command]
pub async fn get_snapshot(runtime: Rt<'_>) -> AppResult<AppSnapshot> {
    Ok((*runtime.snapshot()).clone())
}

#[tauri::command]
pub async fn set_project_override(
    runtime: Rt<'_>,
    project_id: ProjectId,
    overrides: ProjectOverrides,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.set_project_override(project_id, overrides)
    })
    .await
}

#[tauri::command]
pub async fn set_component_kind(
    runtime: Rt<'_>,
    project_id: ProjectId,
    component_key: String,
    kind: Option<ComponentKind>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.set_component_kind(project_id, &component_key, kind)
    })
    .await
}

#[tauri::command]
pub async fn set_project_favorite(
    runtime: Rt<'_>,
    project_id: ProjectId,
    favorite: bool,
) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.set_favorite(project_id, favorite)).await
}

#[tauri::command]
pub async fn ignore_project(
    runtime: Rt<'_>,
    project_id: ProjectId,
    ignored: bool,
) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.set_ignored(project_id, ignored)).await
}

#[tauri::command]
pub async fn forget_project(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.forget_project(project_id)).await
}

#[tauri::command]
pub async fn forget_stopped_projects(runtime: Rt<'_>) -> AppResult<usize> {
    blocking(&runtime, |rt| rt.forget_stopped_projects()).await
}

#[tauri::command]
pub async fn refresh_project(runtime: Rt<'_>, project_id: ProjectId) -> AppResult<()> {
    blocking(&runtime, move |rt| rt.refresh_project(project_id)).await
}

#[tauri::command]
pub async fn list_project_scripts(
    runtime: Rt<'_>,
    project_id: ProjectId,
) -> AppResult<ProjectScripts> {
    blocking(&runtime, move |rt| rt.list_project_scripts(project_id)).await
}

#[tauri::command]
pub async fn set_primary_command(
    runtime: Rt<'_>,
    project_id: ProjectId,
    option: Option<String>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.set_primary_command(project_id, option)
    })
    .await
}

#[tauri::command]
pub async fn dismiss_attention(
    runtime: Rt<'_>,
    project_id: ProjectId,
    attention_id: String,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.dismiss_attention(project_id, &attention_id)
    })
    .await
}

#[tauri::command]
pub async fn set_launch_profiles(
    runtime: Rt<'_>,
    project_id: ProjectId,
    profiles: Vec<LaunchProfileInput>,
) -> AppResult<()> {
    blocking(&runtime, move |rt| {
        rt.set_launch_profiles(project_id, profiles)
    })
    .await
}
