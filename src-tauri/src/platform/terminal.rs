//! "Open Terminal here": the system terminal on macOS, the first known emulator on Linux.

use std::ffi::{OsStr, OsString};
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

/// The command that opens a terminal in `dir` (an existing absolute directory).
///
/// The user PATH is consulted on Linux only, so macOS never pays for the login-shell lookup.
pub(crate) fn command(os: Os, dir: &Path) -> AppResult<CommandSpec> {
    match os {
        Os::MacOs => Ok(mac_command(dir)),
        Os::Linux => linux_command(dir, user_path()),
    }
}

fn mac_command(dir: &Path) -> CommandSpec {
    CommandSpec::new("open", Launch::Helper)
        .arg("-a")
        .arg("Terminal")
        .arg(dir)
        .cwd(dir)
}

fn linux_command(dir: &Path, search_path: &OsStr) -> AppResult<CommandSpec> {
    EMULATORS
        .iter()
        .find_map(|emulator| {
            resolve_program(emulator.bin, search_path)
                .map(|program| emulator_spec(program, emulator, dir))
        })
        .ok_or_else(|| AppError::Unsupported("no supported terminal emulator found".to_string()))
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

    #[test]
    fn macos_opens_terminal_app_at_the_directory() {
        let dir = Path::new("/Users/zoë/Mein Projekt/日本語 app");
        let spec = command(Os::MacOs, dir).unwrap();
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

        let spec = linux_command(&dir, &search).unwrap();
        assert_eq!(spec.program, konsole.into_os_string());
        assert_eq!(spec.args, [os("--workdir"), dir.clone().into_os_string()]);

        std::fs::remove_file(bin.join("konsole")).unwrap();
        let spec = linux_command(&dir, &search).unwrap();
        assert_eq!(spec.program, kitty.into_os_string());

        std::fs::remove_file(bin.join("kitty")).unwrap();
        let spec = linux_command(&dir, &search).unwrap();
        assert_eq!(spec.program, xterm.into_os_string());
        assert!(spec.args.is_empty());
        assert_eq!(spec.cwd, Some(dir));
    }

    #[test]
    fn linux_without_any_emulator_is_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        let err = linux_command(tmp.path(), tmp.path().as_os_str()).unwrap_err();
        assert!(
            matches!(&err, AppError::Unsupported(m) if m == "no supported terminal emulator found"),
            "{err:?}"
        );
    }
}
