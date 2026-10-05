use super::blocking;
use crate::error::AppResult;
use crate::models::{ActivityEvent, ProjectId};
use crate::services::runtime::Runtime;
use std::sync::Arc;
use tauri::State;

type Rt<'a> = State<'a, Arc<Runtime>>;

/// Bounded global activity (newest first). `limit` defaults to the retention cap.
#[tauri::command]
pub async fn list_activity(
    runtime: Rt<'_>,
    project_id: Option<ProjectId>,
    limit: Option<u32>,
) -> AppResult<Vec<ActivityEvent>> {
    let limit = limit.map_or(usize::MAX, |l| l as usize);
    blocking(&runtime, move |rt| Ok(rt.list_activity(project_id, limit))).await
}

#[tauri::command]
pub async fn clear_activity(runtime: Rt<'_>) -> AppResult<()> {
    blocking(&runtime, |rt| rt.clear_activity()).await
}
