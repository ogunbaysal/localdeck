//! Stand-ins for targets other than macOS/Linux: same API, every action is unsupported.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Child;
use std::sync::LazyLock;

use super::Signal;
use crate::error::{AppError, AppResult};
use crate::models::{EditorInfo, PrimaryCommand};

fn unsupported<T>() -> AppResult<T> {
    Err(AppError::Unsupported(
        "LocalDeck supports macOS and Linux only".to_string(),
    ))
}

pub fn send_signal(_pid: u32, _signal: Signal) -> AppResult<()> {
    unsupported()
}

pub fn open_url(_url: &str) -> AppResult<()> {
    unsupported()
}

pub fn open_folder(_path: &Path) -> AppResult<()> {
    unsupported()
}

pub fn detect_editors() -> Vec<EditorInfo> {
    Vec::new()
}

pub fn default_editor_id() -> Option<String> {
    None
}

pub fn open_in_editor(_editor_id: &str, _path: &Path) -> AppResult<()> {
    unsupported()
}

pub fn open_terminal(_path: &Path) -> AppResult<()> {
    unsupported()
}

pub fn spawn_managed(_command: &PrimaryCommand) -> AppResult<Child> {
    unsupported()
}

/// The process `PATH`; there is no login shell to ask on these targets.
pub fn user_path() -> &'static OsStr {
    static PATH: LazyLock<std::ffi::OsString> =
        LazyLock::new(|| std::env::var_os("PATH").unwrap_or_default());
    PATH.as_os_str()
}
