//! "Open Terminal here": the system terminal on macOS, the first known emulator on Linux.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::command::{CommandSpec, Launch, Os};
use super::shell_path::{resolve_program, user_path};
use crate::error::{AppError, AppResult};

/// How an emulator is told which directory to start in.
enum CwdFlag {
    /// A single `--flag=<dir>` argument.
    Joined(&'static str),
    /// Two arguments: `--flag <dir>`.
    Split(&'static str),
    /// No flag: rely on the process working directory.
    Inherit,
}

struct Emulator {
    bin: &'static str,
    /// Subcommand arguments that precede the directory flag.
    leading: &'static [&'static str],
    cwd: CwdFlag,
}

/// Preference order; the first one found on the user PATH wins.
const EMULATORS: [Emulator; 11] = [
    Emulator {
        bin: "gnome-terminal",
        leading: &[],
        cwd: CwdFlag::Joined("--working-directory="),
    },
    Emulator {
        bin: "konsole",
        leading: &[],
        cwd: CwdFlag::Split("--workdir"),
    },
    Emulator {
        bin: "xfce4-terminal",
        leading: &[],
        cwd: CwdFlag::Joined("--working-directory="),
    },
    Emulator {
        bin: "tilix",
        leading: &[],
        cwd: CwdFlag::Joined("--working-directory="),
    },
    Emulator {
        bin: "kitty",
        leading: &[],
        cwd: CwdFlag::Split("--directory"),
    },
    Emulator {
        bin: "alacritty",
        leading: &[],
        cwd: CwdFlag::Split("--working-directory"),
    },
    Emulator {
        bin: "wezterm",
        leading: &["start"],
        cwd: CwdFlag::Split("--cwd"),
    },
    Emulator {
        bin: "foot",
        leading: &[],
        cwd: CwdFlag::Joined("--working-directory="),
    },
    Emulator {
        bin: "terminator",
        leading: &[],
        cwd: CwdFlag::Joined("--working-directory="),
    },
    Emulator {
        bin: "x-terminal-emulator",
        leading: &[],
        cwd: CwdFlag::Inherit,
    },
    Emulator {
        bin: "xterm",
        leading: &[],
        cwd: CwdFlag::Inherit,
    },
];

/// How a macOS terminal app is told which directory to open.
enum MacLaunch {
    /// `open -a <App> <dir>`: the app opens a window for a folder it is given.
    OpenFolder,
    /// `open -n -a <App> --args <leading> <flag> <dir>`: the app needs its own CLI flag.
    NewInstance(&'static [&'static str], CwdFlag),
}

struct MacTerminal {
    id: &'static str,
    name: &'static str,
    app: &'static str,
    launch: MacLaunch,
}

const MAC_TERMINALS: [MacTerminal; 7] = [
    MacTerminal {
        id: "terminal",
        name: "Terminal",
        app: "Terminal",
        launch: MacLaunch::OpenFolder,
    },
    MacTerminal {
        id: "iterm",
        name: "iTerm2",
        app: "iTerm",
        launch: MacLaunch::OpenFolder,
    },
    MacTerminal {
        id: "warp",
        name: "Warp",
        app: "Warp",
        launch: MacLaunch::OpenFolder,
    },
    MacTerminal {
        id: "ghostty",
        name: "Ghostty",
        app: "Ghostty",
        launch: MacLaunch::NewInstance(&[], CwdFlag::Joined("--working-directory=")),
    },
    MacTerminal {
        id: "kitty",
        name: "kitty",
        app: "kitty",
        launch: MacLaunch::NewInstance(&[], CwdFlag::Split("--directory")),
    },
    MacTerminal {
        id: "alacritty",
        name: "Alacritty",
        app: "Alacritty",
        launch: MacLaunch::NewInstance(&[], CwdFlag::Split("--working-directory")),
    },
    MacTerminal {
        id: "wezterm",
        name: "WezTerm",
        app: "WezTerm",
        launch: MacLaunch::NewInstance(&["start"], CwdFlag::Split("--cwd")),
    },
];

fn linux_name(bin: &str) -> &'static str {
    match bin {
        "gnome-terminal" => "GNOME Terminal",
        "konsole" => "Konsole",
        "xfce4-terminal" => "Xfce Terminal",
        "tilix" => "Tilix",
        "kitty" => "kitty",
        "alacritty" => "Alacritty",
        "wezterm" => "WezTerm",
        "foot" => "foot",
        "terminator" => "Terminator",
        "x-terminal-emulator" => "System default terminal",
        _ => "xterm",
    }
}

/// Where each OS looks for terminals.
pub(crate) enum TerminalLocator {
    MacApps(Vec<PathBuf>),
    LinuxBins(OsString),
}

impl TerminalLocator {
    pub(crate) fn for_os(os: Os) -> Self {
        match os {
            Os::MacOs => {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                let mut dirs = super::editors::mac_app_dirs(home.as_deref());
                dirs.push(PathBuf::from("/Applications/Utilities"));
                dirs.push(PathBuf::from("/System/Applications/Utilities"));
                Self::MacApps(dirs)
            }
            Os::Linux => Self::LinuxBins(user_path().to_os_string()),
        }
    }

    fn has_mac_app(&self, app: &str) -> bool {
        match self {
            Self::MacApps(dirs) => {
                let bundle = format!("{app}.app");
                dirs.iter().any(|d| d.join(&bundle).is_dir())
            }
            Self::LinuxBins(_) => false,
        }
    }

    fn linux_program(&self, bin: &str) -> Option<PathBuf> {
        match self {
            Self::LinuxBins(path) => resolve_program(bin, path),
            Self::MacApps(_) => None,
        }
    }
}

/// Every terminal for `os` in a stable order, flagged by availability.
pub(crate) fn catalogue(os: Os, locator: &TerminalLocator) -> Vec<crate::models::TerminalInfo> {
    match os {
        Os::MacOs => MAC_TERMINALS
            .iter()
            .map(|t| crate::models::TerminalInfo {
                id: t.id.to_string(),
                name: t.name.to_string(),
                available: locator.has_mac_app(t.app),
            })
            .collect(),
        Os::Linux => EMULATORS
            .iter()
            .map(|e| crate::models::TerminalInfo {
                id: e.bin.to_string(),
                name: linux_name(e.bin).to_string(),
                available: locator.linux_program(e.bin).is_some(),
            })
            .collect(),
    }
}

/// The command that opens a terminal in `dir` (an existing absolute directory).
/// `terminal_id: None` is the platform default; an unknown or missing choice is
/// an error, never a silent substitute.
pub(crate) fn command(
    os: Os,
    dir: &Path,
    terminal_id: Option<&str>,
    locator: &TerminalLocator,
) -> AppResult<CommandSpec> {
    match os {
        Os::MacOs => mac_command(dir, terminal_id.unwrap_or("terminal"), locator),
        Os::Linux => match terminal_id {
            None => linux_default(dir, locator),
            Some(id) => linux_chosen(dir, id, locator),
        },
    }
}

fn mac_command(dir: &Path, id: &str, locator: &TerminalLocator) -> AppResult<CommandSpec> {
    let terminal = MAC_TERMINALS
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown terminal `{id}`")))?;
    if !locator.has_mac_app(terminal.app) {
        return Err(AppError::Unsupported(format!(
            "{} was not found on this machine. Choose another terminal in Settings.",
            terminal.name
        )));
    }
    let spec = CommandSpec::new("open", Launch::Helper).cwd(dir);
    Ok(match &terminal.launch {
        MacLaunch::OpenFolder => spec.arg("-a").arg(terminal.app).arg(dir),
        MacLaunch::NewInstance(leading, flag) => {
            let mut spec = spec.arg("-n").arg("-a").arg(terminal.app).arg("--args");
            for arg in *leading {
                spec = spec.arg(arg);
            }
            match flag {
                CwdFlag::Joined(prefix) => {
                    let mut joined = OsString::from(prefix);
                    joined.push(dir);
                    spec.arg(joined)
                }
                CwdFlag::Split(name) => spec.arg(name).arg(dir),
                CwdFlag::Inherit => spec,
            }
        }
    })
}

fn linux_default(dir: &Path, locator: &TerminalLocator) -> AppResult<CommandSpec> {
    EMULATORS
        .iter()
        .find_map(|emulator| {
            locator
                .linux_program(emulator.bin)
                .map(|program| emulator_spec(program, emulator, dir))
        })
        .ok_or_else(|| AppError::Unsupported("no supported terminal emulator found".to_string()))
}

fn linux_chosen(dir: &Path, id: &str, locator: &TerminalLocator) -> AppResult<CommandSpec> {
    let emulator = EMULATORS
        .iter()
        .find(|e| e.bin == id)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown terminal `{id}`")))?;
    let program = locator.linux_program(emulator.bin).ok_or_else(|| {
        AppError::Unsupported(format!(
            "{} was not found on this machine. Choose another terminal in Settings.",
            linux_name(emulator.bin)
        ))
    })?;
    Ok(emulator_spec(program, emulator, dir))
}

fn emulator_spec(program: PathBuf, emulator: &Emulator, dir: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new(program, Launch::Detached).cwd(dir);
    for arg in emulator.leading {
        spec = spec.arg(arg);
    }
    match emulator.cwd {
        CwdFlag::Joined(flag) => {
            let mut joined = OsString::from(flag);
            joined.push(dir);
            spec.arg(joined)
        }
        CwdFlag::Split(flag) => spec.arg(flag).arg(dir),
        CwdFlag::Inherit => spec,
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

    /// A fake Applications folder containing the given `.app` bundles.
    fn mac_locator(apps: &[&str]) -> TerminalLocator {
        let dir = tempfile::tempdir().unwrap().keep();
        for app in apps {
            std::fs::create_dir_all(dir.join(format!("{app}.app"))).unwrap();
        }
        TerminalLocator::MacApps(vec![dir])
    }

    #[test]
    fn macos_opens_terminal_app_at_the_directory() {
        let dir = Path::new("/Users/zoë/Mein Projekt/日本語 app");
        let spec = command(Os::MacOs, dir, None, &mac_locator(&["Terminal"])).unwrap();
        assert_eq!(spec.program, os("open"));
        assert_eq!(
            spec.args,
            [
                os("-a"),
                os("Terminal"),
                os("/Users/zoë/Mein Projekt/日本語 app")
            ]
        );
        assert_eq!(spec.cwd.as_deref(), Some(dir));
        assert_eq!(spec.launch, Launch::Helper);
    }

    #[test]
    fn every_linux_emulator_gets_its_own_directory_flags() {
        let dir = Path::new("/home/zoë/my projects/café ☕");
        let expected: [(&str, Vec<&str>); 11] = [
            (
                "gnome-terminal",
                vec!["--working-directory=/home/zoë/my projects/café ☕"],
            ),
            (
                "konsole",
                vec!["--workdir", "/home/zoë/my projects/café ☕"],
            ),
            (
                "xfce4-terminal",
                vec!["--working-directory=/home/zoë/my projects/café ☕"],
            ),
            (
                "tilix",
                vec!["--working-directory=/home/zoë/my projects/café ☕"],
            ),
            (
                "kitty",
                vec!["--directory", "/home/zoë/my projects/café ☕"],
            ),
            (
                "alacritty",
                vec!["--working-directory", "/home/zoë/my projects/café ☕"],
            ),
            (
                "wezterm",
                vec!["start", "--cwd", "/home/zoë/my projects/café ☕"],
            ),
            (
                "foot",
                vec!["--working-directory=/home/zoë/my projects/café ☕"],
            ),
            (
                "terminator",
                vec!["--working-directory=/home/zoë/my projects/café ☕"],
            ),
            ("x-terminal-emulator", vec![]),
            ("xterm", vec![]),
        ];
        assert_eq!(EMULATORS.len(), expected.len());
        for (emulator, (bin, args)) in EMULATORS.iter().zip(expected) {
            assert_eq!(emulator.bin, bin);
            let program = PathBuf::from("/usr/bin").join(bin);
            let spec = emulator_spec(program.clone(), emulator, dir);
            assert_eq!(spec.program, program.into_os_string());
            assert_eq!(
                spec.args,
                args.into_iter().map(os).collect::<Vec<_>>(),
                "{bin}"
            );
            assert_eq!(spec.cwd.as_deref(), Some(dir), "{bin} must also set cwd");
            assert_eq!(spec.launch, Launch::Detached);
        }
    }

    #[test]
    fn linux_picks_the_first_available_emulator_in_preference_order() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bïn dir");
        let xterm = executable(&bin, "xterm");
        let kitty = executable(&bin, "kitty");
        let konsole = executable(&bin, "konsole");
        plain_file(&bin, "gnome-terminal"); // not executable: skipped
        let dir = tmp.path().join("proj ünï");
        std::fs::create_dir(&dir).unwrap();
        let search = env::join_paths([&bin]).unwrap();

        let spec = linux_default(&dir, &TerminalLocator::LinuxBins(search.clone())).unwrap();
        assert_eq!(spec.program, konsole.into_os_string());
        assert_eq!(spec.args, [os("--workdir"), dir.clone().into_os_string()]);

        std::fs::remove_file(bin.join("konsole")).unwrap();
        let spec = linux_default(&dir, &TerminalLocator::LinuxBins(search.clone())).unwrap();
        assert_eq!(spec.program, kitty.into_os_string());

        std::fs::remove_file(bin.join("kitty")).unwrap();
        let spec = linux_default(&dir, &TerminalLocator::LinuxBins(search.clone())).unwrap();
        assert_eq!(spec.program, xterm.into_os_string());
        assert!(spec.args.is_empty());
        assert_eq!(spec.cwd, Some(dir));
    }

    #[test]
    fn linux_without_any_emulator_is_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        let err = linux_default(
            tmp.path(),
            &TerminalLocator::LinuxBins(tmp.path().as_os_str().to_os_string()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, AppError::Unsupported(m) if m == "no supported terminal emulator found"),
            "{err:?}"
        );
    }

    #[test]
    fn every_mac_terminal_gets_its_own_launch_recipe() {
        let dir = Path::new("/Users/zoë/Mein Projekt/日本語 app");
        let all: Vec<&str> = MAC_TERMINALS.iter().map(|t| t.app).collect();
        let locator = mac_locator(&all);
        let expected: [(&str, Vec<&str>); 7] = [
            ("terminal", vec!["-a", "Terminal", "<dir>"]),
            ("iterm", vec!["-a", "iTerm", "<dir>"]),
            ("warp", vec!["-a", "Warp", "<dir>"]),
            (
                "ghostty",
                vec!["-n", "-a", "Ghostty", "--args", "--working-directory=<dir>"],
            ),
            (
                "kitty",
                vec!["-n", "-a", "kitty", "--args", "--directory", "<dir>"],
            ),
            (
                "alacritty",
                vec![
                    "-n",
                    "-a",
                    "Alacritty",
                    "--args",
                    "--working-directory",
                    "<dir>",
                ],
            ),
            (
                "wezterm",
                vec!["-n", "-a", "WezTerm", "--args", "start", "--cwd", "<dir>"],
            ),
        ];
        for (id, args) in expected {
            let spec = command(Os::MacOs, dir, Some(id), &locator).unwrap();
            let want: Vec<OsString> = args
                .iter()
                .map(|a| os(&a.replace("<dir>", &dir.to_string_lossy())))
                .collect();
            assert_eq!(spec.program, os("open"), "{id}");
            assert_eq!(spec.args, want, "{id}");
            assert_eq!(spec.cwd.as_deref(), Some(dir));
        }
    }

    #[test]
    fn unknown_and_missing_terminals_are_errors_not_substitutions() {
        let dir = Path::new("/tmp");
        let only_terminal = mac_locator(&["Terminal"]);
        assert!(matches!(
            command(Os::MacOs, dir, Some("hyper"), &only_terminal),
            Err(AppError::InvalidInput(_))
        ));
        let missing = command(Os::MacOs, dir, Some("iterm"), &only_terminal).unwrap_err();
        assert!(
            matches!(&missing, AppError::Unsupported(m) if m.contains("iTerm2")),
            "{missing:?}"
        );

        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        executable(&bin, "xterm");
        let linux = TerminalLocator::LinuxBins(env::join_paths([&bin]).unwrap());
        assert!(matches!(
            command(Os::Linux, dir, Some("kitty"), &linux),
            Err(AppError::Unsupported(_))
        ));
        assert!(matches!(
            command(Os::Linux, dir, Some("rm -rf"), &linux),
            Err(AppError::InvalidInput(_))
        ));
        let chosen = command(Os::Linux, dir, Some("xterm"), &linux).unwrap();
        assert_eq!(chosen.launch, Launch::Detached);
    }

    #[test]
    fn catalogues_flag_availability_in_stable_order() {
        let mac = catalogue(Os::MacOs, &mac_locator(&["Terminal", "Ghostty"]));
        let ids: Vec<&str> = mac.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "terminal",
                "iterm",
                "warp",
                "ghostty",
                "kitty",
                "alacritty",
                "wezterm"
            ]
        );
        let available: Vec<&str> = mac
            .iter()
            .filter(|t| t.available)
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(available, ["terminal", "ghostty"]);

        let tmp = tempfile::tempdir().unwrap();
        executable(tmp.path(), "konsole");
        let linux = catalogue(
            Os::Linux,
            &TerminalLocator::LinuxBins(env::join_paths([tmp.path()]).unwrap()),
        );
        assert_eq!(linux.len(), 11);
        assert_eq!(
            linux
                .iter()
                .filter(|t| t.available)
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["konsole"]
        );
    }
}
