//! OS integration for macOS and Linux: signals, opening URLs/folders/editors/terminals, the
//! user's real `PATH`, and spawning LocalDeck-managed commands.
//!
//! OS differences stay inside this module, behind pure command builders that take an `Os`
//! value; callers never use `cfg`. Targets other than macOS/Linux compile to stubs that return
//! `AppError::Unsupported`.

#[cfg(unix)]
mod api;
#[cfg(unix)]
mod command;
#[cfg(unix)]
mod editors;
#[cfg(unix)]
mod managed;
#[cfg(unix)]
mod shell_path;
#[cfg(unix)]
mod signals;
#[cfg(unix)]
mod terminal;
#[cfg(not(unix))]
mod unsupported;

#[cfg(unix)]
pub use api::*;
#[cfg(not(unix))]
pub use unsupported::*;

/// Signals LocalDeck may send to a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// `SIGTERM`: ask the process to shut down gracefully.
    Terminate,
    /// `SIGKILL`: force kill, only after explicit user action.
    Kill,
}

#[cfg(all(test, unix))]
mod test_util {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    /// Creates `dir/name` (and `dir`) as an executable file. Only ever stat'ed, never run.
    pub fn executable(dir: &Path, name: &str) -> PathBuf {
        let path = plain_file(dir, name);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Creates `dir/name` (and `dir`) as a non-executable file.
    pub fn plain_file(dir: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        path
    }
}
