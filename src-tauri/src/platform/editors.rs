//! Static editor catalogue, detection, and launch commands.
//!
//! Only entries from [`CATALOGUE`] are ever launched; nothing user-supplied is executed.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::command::{CommandSpec, Launch, Os};
use super::shell_path::{home_dir, join_dirs, merge_paths, resolve_program, user_path};

pub(crate) struct Editor {
    pub id: &'static str,
    pub name: &'static str,
    /// macOS application names tried in order (`<name>.app`, launched with `open -a`).
    mac_apps: &'static [&'static str],
    /// Linux executable names searched on the user PATH.
    linux_bins: &'static [&'static str],
}

/// Detection and display order.
pub(crate) const CATALOGUE: [Editor; 9] = [
    Editor {
        id: "vscode",
        name: "Visual Studio Code",
        mac_apps: &["Visual Studio Code"],
        linux_bins: &["code"],
    },
    Editor {
        id: "cursor",
        name: "Cursor",
        mac_apps: &["Cursor"],
        linux_bins: &["cursor"],
    },
    Editor {
        id: "zed",
        name: "Zed",
        mac_apps: &["Zed"],
        linux_bins: &["zed"],
    },
    Editor {
        id: "webstorm",
        name: "WebStorm",
        mac_apps: &["WebStorm"],
        linux_bins: &["webstorm"],
    },
    Editor {
        id: "idea",
        name: "IntelliJ IDEA",
        mac_apps: &["IntelliJ IDEA", "IntelliJ IDEA CE"],
        linux_bins: &["idea"],
    },
    Editor {
        id: "phpstorm",
        name: "PhpStorm",
        mac_apps: &["PhpStorm"],
        linux_bins: &["phpstorm"],
    },
    Editor {
        id: "pycharm",
        name: "PyCharm",
        mac_apps: &["PyCharm", "PyCharm CE"],
        linux_bins: &["pycharm"],
    },
    Editor {
        id: "rustrover",
        name: "RustRover",
        mac_apps: &["RustRover"],
        linux_bins: &["rustrover"],
    },
    Editor {
        id: "goland",
        name: "GoLand",
        mac_apps: &["GoLand"],
        linux_bins: &["goland"],
    },
];

pub(crate) fn find(id: &str) -> Option<&'static Editor> {
    CATALOGUE.iter().find(|editor| editor.id == id)
}

/// Where to look for editors on this machine; built once per detection pass.
pub(crate) enum Locator {
    /// Directories that may contain `<App>.app` bundles.
    MacApps(Vec<PathBuf>),
    /// `PATH`-style search list for editor executables.
    LinuxBins(OsString),
}

impl Locator {
    pub(crate) fn for_os(os: Os) -> Self {
        let home = home_dir();
        match os {
            Os::MacOs => Self::MacApps(mac_app_dirs(home.as_deref())),
            Os::Linux => Self::LinuxBins(merge_paths(&[
                user_path(),
                &join_dirs(&linux_extra_dirs(home.as_deref())),
            ])),
        }
    }

    /// The launcher for `editor`: the macOS app name or the Linux executable path.
    pub(crate) fn locate(&self, editor: &Editor) -> Option<OsString> {
        match self {
            Self::MacApps(dirs) => editor
                .mac_apps
                .iter()
                .find(|app| {
                    let bundle = format!("{app}.app");
                    dirs.iter().any(|dir| dir.join(&bundle).is_dir())
                })
                .map(OsString::from),
            Self::LinuxBins(search_path) => editor
                .linux_bins
                .iter()
                .find_map(|bin| resolve_program(bin, search_path))
                .map(PathBuf::into_os_string),
        }
    }
}

pub(crate) fn mac_app_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(home) = home {
        dirs.push(home.join("Applications"));
        dirs.push(home.join("Applications/JetBrains Toolbox"));
    }
    dirs.push(PathBuf::from("/System/Applications"));
    dirs
}

/// Places editor launchers commonly live that a minimal PATH misses.
fn linux_extra_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home {
        dirs.push(home.join(".local/share/JetBrains/Toolbox/scripts"));
    }
    dirs.push(PathBuf::from("/snap/bin"));
    if let Some(home) = home {
        dirs.push(home.join(".local/bin"));
    }
    dirs
}

/// macOS: `open -a <app> <path>`. Linux: `<bin> <path>`, detached. `path` must be absolute.
pub(crate) fn editor_command(os: Os, launcher: &OsStr, path: &Path) -> CommandSpec {
    match os {
        Os::MacOs => CommandSpec::new("open", Launch::Helper)
            .arg("-a")
            .arg(launcher)
            .arg(path),
        Os::Linux => CommandSpec::new(launcher, Launch::Detached).arg(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::test_util::{executable, plain_file};
    use std::env;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    fn editor(id: &str) -> &'static Editor {
        find(id).unwrap_or_else(|| panic!("{id} not in catalogue"))
    }

    #[test]
    fn catalogue_order_ids_and_names_are_stable() {
        let listed: Vec<_> = CATALOGUE.iter().map(|e| (e.id, e.name)).collect();
        assert_eq!(
            listed,
            [
                ("vscode", "Visual Studio Code"),
                ("cursor", "Cursor"),
                ("zed", "Zed"),
                ("webstorm", "WebStorm"),
                ("idea", "IntelliJ IDEA"),
                ("phpstorm", "PhpStorm"),
                ("pycharm", "PyCharm"),
                ("rustrover", "RustRover"),
                ("goland", "GoLand"),
            ]
        );
    }

    #[test]
    fn find_matches_ids_exactly_and_rejects_unknown() {
        assert_eq!(find("zed").map(|e| e.name), Some("Zed"));
        for id in ["", "Zed", "VSCODE", "code", "vscode ", "../vscode", "sh"] {
            assert!(find(id).is_none(), "{id:?}");
        }
    }

    #[test]
    fn mac_detection_finds_apps_in_any_applications_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let system = tmp.path().join("Applications");
        let user = tmp.path().join("Users/zoë/Applications");
        std::fs::create_dir_all(system.join("Visual Studio Code.app")).unwrap();
        std::fs::create_dir_all(user.join("Zed.app")).unwrap();
        std::fs::create_dir_all(user.join("IntelliJ IDEA CE.app")).unwrap();
        plain_file(&system, "Cursor.app"); // a file is not an app bundle
        let locator = Locator::MacApps(vec![system, user]);

        assert_eq!(
            locator.locate(editor("vscode")),
            Some(os("Visual Studio Code"))
        );
        assert_eq!(locator.locate(editor("zed")), Some(os("Zed")));
        assert_eq!(locator.locate(editor("idea")), Some(os("IntelliJ IDEA CE")));
        assert_eq!(locator.locate(editor("cursor")), None);
        assert_eq!(locator.locate(editor("goland")), None);
    }

    #[test]
    fn mac_detection_prefers_the_first_listed_app_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("PyCharm CE.app")).unwrap();
        std::fs::create_dir_all(tmp.path().join("PyCharm.app")).unwrap();
        let locator = Locator::MacApps(vec![tmp.path().to_path_buf()]);
        assert_eq!(locator.locate(editor("pycharm")), Some(os("PyCharm")));
    }

    #[test]
    fn linux_detection_finds_executables_on_the_search_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bïn dir 日本");
        let code = executable(&bin, "code");
        plain_file(&bin, "cursor"); // present but not executable
        executable(&bin, "idea");
        let locator = Locator::LinuxBins(env::join_paths([&bin]).unwrap());

        assert_eq!(
            locator.locate(editor("vscode")),
            Some(code.into_os_string())
        );
        assert!(locator.locate(editor("idea")).is_some());
        assert_eq!(locator.locate(editor("cursor")), None);
        assert_eq!(locator.locate(editor("zed")), None);
    }

    #[test]
    fn search_locations_include_toolbox_snap_and_user_bin_dirs() {
        let home = Path::new("/home/zoë");
        assert_eq!(
            linux_extra_dirs(Some(home)),
            [
                "/home/zoë/.local/share/JetBrains/Toolbox/scripts",
                "/snap/bin",
                "/home/zoë/.local/bin"
            ]
            .map(PathBuf::from)
        );
        assert_eq!(linux_extra_dirs(None), [PathBuf::from("/snap/bin")]);
        assert_eq!(
            mac_app_dirs(Some(home)),
            [
                "/Applications",
                "/home/zoë/Applications",
                "/home/zoë/Applications/JetBrains Toolbox",
                "/System/Applications"
            ]
            .map(PathBuf::from)
        );
        assert_eq!(mac_app_dirs(None).len(), 2);
    }

    #[test]
    fn editor_command_uses_open_dash_a_on_macos() {
        let path = Path::new("/Users/zoë/My Projects/café app");
        let spec = editor_command(Os::MacOs, OsStr::new("Visual Studio Code"), path);
        assert_eq!(spec.program, os("open"));
        assert_eq!(
            spec.args,
            [
                os("-a"),
                os("Visual Studio Code"),
                os("/Users/zoë/My Projects/café app")
            ]
        );
        assert_eq!(spec.launch, Launch::Helper);
        assert_eq!(spec.cwd, None);
    }

    #[test]
    fn editor_command_spawns_the_binary_detached_on_linux() {
        let path = Path::new("/home/zoë/my projects/日本");
        let spec = editor_command(Os::Linux, OsStr::new("/snap/bin/code"), path);
        assert_eq!(spec.program, os("/snap/bin/code"));
        assert_eq!(spec.args, [os("/home/zoë/my projects/日本")]);
        assert_eq!(spec.launch, Launch::Detached);
    }
}
