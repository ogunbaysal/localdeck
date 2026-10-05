//! Narrow, intention-revealing Tauri commands. There is deliberately no generic
//! "execute this string" command: every command maps to one product action and
//! resolves paths/commands from Rust-owned state, never from frontend input.

mod actions;
mod activity;
mod processes;
mod projects;
mod settings;
mod transfer;

pub use actions::*;
pub use activity::*;
pub use processes::*;
pub use projects::*;
pub use settings::*;
pub use transfer::*;

use crate::error::{AppError, AppResult};
use crate::services::runtime::Runtime;
use std::sync::Arc;

/// Runs possibly-blocking service work off the async executor threads.
pub(crate) async fn blocking<T, F>(runtime: &Arc<Runtime>, work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Runtime) -> AppResult<T> + Send + 'static,
{
    let runtime = runtime.clone();
    tauri::async_runtime::spawn_blocking(move || work(&runtime))
        .await
        .map_err(|err| AppError::Internal(err.to_string()))?
}
