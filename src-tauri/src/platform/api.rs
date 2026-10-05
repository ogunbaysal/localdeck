//! Public platform actions for macOS and Linux. Callers never branch on the OS.

use std::ffi::OsStr;
use std::path::{self, Path, PathBuf};
use std::process::Child;

use super::command::{self, Os};
use super::editors::{self, Locator};
use super::{managed, terminal};
use crate::error::{AppError, AppResult};
use crate::models::{EditorInfo, PrimaryCommand, TerminalInfo};

pub use super::shell_path::user_path;
pub use super::signals::send_signal;

/// Opens an `http`/`https` URL in the default browser.
pub fn open_url(url: &str) -> AppResult<()> {
    command::validate_http_url(url)?;
    command::opener_command(Os::current()?, OsStr::new(url)).run()
}

/// Reveals a directory in the system file manager.
pub fn open_folder(path: &Path) -> AppResult<()> {
    let dir = absolute_existing(path, true)?;
    command::opener_command(Os::current()?, dir.as_os_str()).run()
}

/// Every editor LocalDeck knows, in a stable order, flagged by whether it is installed.
/// On Linux the first call may block while the user PATH is resolved.
pub fn detect_editors() -> Vec<EditorInfo> {
    let locator = Os::current().ok().map(Locator::for_os);
    editors::CATALOGUE
        .iter()
        .map(|editor| EditorInfo {
            id: editor.id.to_string(),
            name: editor.name.to_string(),
            available: locator
                .as_ref()
                .is_some_and(|locator| locator.locate(editor).is_some()),
        })
        .collect()
}

/// The first installed editor in catalogue order.
pub fn default_editor_id() -> Option<String> {
    let locator = Locator::for_os(Os::current().ok()?);
    editors::CATALOGUE
        .iter()
        .find(|editor| locator.locate(editor).is_some())
        .map(|editor| editor.id.to_string())
}

/// Opens a file or directory in one of the catalogue editors.
pub fn open_in_editor(editor_id: &str, path: &Path) -> AppResult<()> {
    let editor = editors::find(editor_id)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown editor `{editor_id}`")))?;
    let target = absolute_existing(path, false)?;
    let os = Os::current()?;
    let launcher = Locator::for_os(os).locate(editor).ok_or_else(|| {
        AppError::EditorNotFound(format!(
            "{} was not found on this machine. Choose another editor in Settings.",
            editor.name
        ))
    })?;
    editors::editor_command(os, &launcher, &target).run()
}

/// Every terminal LocalDeck knows for this OS, flagged by whether it is installed.
pub fn detect_terminals() -> Vec<TerminalInfo> {
    match Os::current() {
        Ok(os) => terminal::catalogue(os, &terminal::TerminalLocator::for_os(os)),
        Err(_) => Vec::new(),
    }
}

/// Opens a terminal in `path`: the chosen one (`terminal_id`), or the platform
/// default. An unknown or missing terminal is an error, never a substitute.
pub fn open_terminal(path: &Path, terminal_id: Option<&str>) -> AppResult<()> {
    let dir = absolute_existing(path, true)?;
    let os = Os::current()?;
    terminal::command(
        os,
        &dir,
        terminal_id,
        &terminal::TerminalLocator::for_os(os),
    )?
    .run()
}

/// Starts a LocalDeck-managed command with the user's `PATH`. The caller owns the returned child
/// and must `wait()` it. May block on first use while the user PATH is resolved.
pub fn spawn_managed(command: &PrimaryCommand, env: &[(String, String)]) -> AppResult<Child> {
    managed::spawn(command, env, user_path())
}

/// `path` as an absolute path, if it exists (and is a directory when `dir_only`). Absolute paths
/// can never be mistaken for command-line options by the helpers we hand them to.
fn absolute_existing(path: &Path, dir_only: bool) -> AppResult<PathBuf> {
    let exists = if dir_only {
        path.is_dir()
    } else {
        path.exists()
    };
    if !exists {
        return Err(AppError::PathNotFound(path.display().to_string()));
    }
    path::absolute(path)
        .map_err(|err| AppError::InvalidInput(format!("Invalid path {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_path_not_found(result: AppResult<()>) -> bool {
        matches!(result, Err(AppError::PathNotFound(_)))
    }

    #[test]
    fn open_url_rejects_everything_but_http_urls_before_launching() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "mailto:me@example.com",
            "http://",
            "",
            "garbage",
        ] {
            assert!(
                matches!(open_url(url), Err(AppError::InvalidInput(_))),
                "{url:?}"
            );
        }
    }

    #[test]
    fn path_actions_report_missing_paths_before_launching_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nö such dir");
        let file = tmp.path().join("a file");
        std::fs::write(&file, "").unwrap();

        assert!(is_path_not_found(open_folder(&missing)));
        assert!(is_path_not_found(open_folder(&file)));
        assert!(is_path_not_found(open_terminal(&missing, None)));
        assert!(is_path_not_found(open_terminal(&file, None)));
        assert!(is_path_not_found(open_in_editor("vscode", &missing)));
        assert!(is_path_not_found(open_in_editor("zed", Path::new(""))));
    }

    #[test]
    fn unknown_editor_ids_are_invalid_input_even_for_existing_paths() {
        let tmp = tempfile::tempdir().unwrap();
        for id in ["", "emacs", "sh", "VSCODE", "code"] {
            let err = open_in_editor(id, tmp.path()).unwrap_err();
            assert!(matches!(err, AppError::InvalidInput(_)), "{id:?}: {err:?}");
        }
    }

    #[test]
    fn absolute_existing_makes_paths_absolute_and_enforces_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("f.txt");
        std::fs::write(&file, "").unwrap();

        assert_eq!(absolute_existing(tmp.path(), true).unwrap(), tmp.path());
        assert_eq!(absolute_existing(&file, false).unwrap(), file);
        assert!(is_path_not_found(absolute_existing(&file, true).map(drop)));

        let relative = absolute_existing(Path::new("."), true).unwrap();
        assert!(relative.is_absolute());
        let dash = tmp.path().join("-weird dir");
        std::fs::create_dir(&dash).unwrap();
        assert!(absolute_existing(&dash, true).unwrap().is_absolute());
    }

    #[test]
    fn detection_lists_the_whole_catalogue_and_default_is_its_first_available() {
        let detected = detect_editors();
        let ids: Vec<_> = detected.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "vscode",
                "cursor",
                "zed",
                "webstorm",
                "idea",
                "phpstorm",
                "pycharm",
                "rustrover",
                "goland"
            ]
        );
        assert!(detected.iter().all(|e| !e.name.is_empty()));
        let first_available = detected.iter().find(|e| e.available).map(|e| e.id.clone());
        assert_eq!(default_editor_id(), first_available);
    }
}
