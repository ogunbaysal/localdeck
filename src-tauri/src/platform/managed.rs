//! Spawning LocalDeck-managed start commands.

use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{self, Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::shell_path::{is_executable, resolve_program};
use crate::error::{AppError, AppResult};
use crate::models::PrimaryCommand;

/// Starts `command` as `program + args[]` in `command.cwd` (never through a shell) with
/// `path_var` as its `PATH`. The child gets its own process group, so it outlives LocalDeck and is
/// not hit by signals aimed at LocalDeck's group, like a job started from a terminal.
pub(crate) fn spawn(command: &PrimaryCommand, path_var: &OsStr) -> AppResult<Child> {
    let cwd = Path::new(&command.cwd);
    if !cwd.is_dir() {
        return Err(AppError::PathNotFound(command.cwd.clone()));
    }
    let program = resolve(&command.program, cwd, path_var)?;
    Command::new(program)
        .args(&command.args)
        .current_dir(cwd)
        .env("PATH", path_var)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| {
            AppError::CommandSpawnFailed(format!(
                "`{}` could not be started: {err}",
                command.program
            ))
        })
}

/// Names are searched on `path_var`. Anything with a `/` is used as given, relative paths being
/// anchored at the command's working directory (where the child will start).
fn resolve(program: &str, cwd: &Path, path_var: &OsStr) -> AppResult<PathBuf> {
    if program.is_empty() {
        return Err(AppError::CommandSpawnFailed(
            "the start command has no program".to_string(),
        ));
    }
    let resolved = if program.contains('/') {
        path::absolute(cwd.join(program))
            .ok()
            .filter(|candidate| is_executable(candidate))
    } else {
        resolve_program(program, path_var)
    };
    resolved.ok_or_else(|| {
        let place = if program.contains('/') {
            "does not exist or is not executable"
        } else {
            "was not found on PATH"
        };
        AppError::CommandSpawnFailed(format!(
            "`{program}` {place}. Install it or check your shell configuration, then try again."
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    const SYSTEM_PATH: &str = "/bin:/usr/bin";

    fn command(program: &str, args: &[&str], cwd: &Path) -> PrimaryCommand {
        PrimaryCommand {
            program: program.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            cwd: cwd.to_str().unwrap().to_string(),
            label: format!("{program} {}", args.join(" ")),
            script: None,
        }
    }

    fn stop(mut child: Child) {
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    fn pgid(child: &Child) -> libc::pid_t {
        // SAFETY: plain integer syscall.
        unsafe { libc::getpgid(child.id() as libc::pid_t) }
    }

    #[test]
    fn spawns_in_its_own_process_group_inside_a_cwd_with_spaces_and_non_ascii() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("my proj ünï 日本");
        std::fs::create_dir(&cwd).unwrap();
        let child = spawn(&command("sleep", &["30"], &cwd), OsStr::new(SYSTEM_PATH)).unwrap();
        assert_eq!(pgid(&child), child.id() as libc::pid_t);
        assert_ne!(pgid(&child), unsafe { libc::getpgrp() });
        stop(child);
    }

    #[test]
    fn child_runs_in_cwd_with_the_given_path_and_null_stdio() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("proj ünï");
        std::fs::create_dir(&cwd).unwrap();
        let path_var = format!("{SYSTEM_PATH}:/custom dir/ü");
        let script = r#"printf '%s\n%s\n' "$PATH" "$(pwd -P)" > out.txt; echo to-stdout; echo to-stderr >&2; read line || echo eof >> out.txt"#;
        let mut child =
            spawn(&command("sh", &["-c", script], &cwd), OsStr::new(&path_var)).unwrap();
        assert!(child.wait().unwrap().success());
        let out = std::fs::read_to_string(cwd.join("out.txt")).unwrap();
        let mut lines = out.lines();
        assert_eq!(lines.next(), Some(path_var.as_str()));
        assert_eq!(
            lines
                .next()
                .map(Path::new)
                .map(|p| p.canonicalize().unwrap()),
            Some(cwd.canonicalize().unwrap())
        );
        assert_eq!(lines.next(), Some("eof"), "stdin must be /dev/null");
        assert!(child.stdout.is_none() && child.stderr.is_none() && child.stdin.is_none());
    }

    #[test]
    fn arguments_are_passed_verbatim_without_shell_interpretation() {
        let tmp = tempfile::tempdir().unwrap();
        let arg = "a b; touch pwned $(touch pwned) `x` ünï*";
        let mut child = spawn(
            &command(
                "sh",
                &["-c", r#"printf %s "$0" > arg.txt"#, arg],
                tmp.path(),
            ),
            OsStr::new(SYSTEM_PATH),
        )
        .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("arg.txt")).unwrap(),
            arg
        );
        assert!(!tmp.path().join("pwned").exists());
    }

    #[test]
    fn relative_program_paths_resolve_against_the_command_cwd() {
        let mut child = spawn(
            &command("./sleep", &["30"], Path::new("/bin")),
            OsStr::new(""),
        )
        .unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        let child = spawn(
            &command("/bin/sleep", &["30"], Path::new("/")),
            OsStr::new(""),
        )
        .unwrap();
        stop(child);
    }

    #[test]
    fn missing_cwd_is_path_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("gone ünï");
        let err = spawn(&command("sleep", &["1"], &missing), OsStr::new(SYSTEM_PATH)).unwrap_err();
        assert!(
            matches!(&err, AppError::PathNotFound(p) if Path::new(p) == missing),
            "{err:?}"
        );

        let file = tmp.path().join("file.txt");
        std::fs::write(&file, "").unwrap();
        let err = spawn(&command("sleep", &["1"], &file), OsStr::new(SYSTEM_PATH)).unwrap_err();
        assert!(matches!(err, AppError::PathNotFound(_)));
    }

    #[test]
    fn unknown_or_non_executable_programs_are_spawn_failures() {
        let cwd = Path::new("/");
        let err = spawn(
            &command("definitely-not-a-real-bun", &[], cwd),
            OsStr::new(SYSTEM_PATH),
        )
        .unwrap_err();
        assert!(
            matches!(&err, AppError::CommandSpawnFailed(m) if m.starts_with("`definitely-not-a-real-bun` was not found on PATH")),
            "{err:?}"
        );

        for program in [
            "",
            "./no-such-file",
            "/definitely/not/here",
            "/etc/hosts",
            "/etc",
        ] {
            let err = spawn(&command(program, &[], cwd), OsStr::new(SYSTEM_PATH)).unwrap_err();
            assert!(
                matches!(err, AppError::CommandSpawnFailed(_)),
                "{program:?}: {err:?}"
            );
        }
    }

    #[test]
    fn embedded_nul_in_arguments_is_a_spawn_failure() {
        let err = spawn(
            &command("sleep", &["1\0 2"], Path::new("/")),
            OsStr::new(SYSTEM_PATH),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::CommandSpawnFailed(_)), "{err:?}");
    }
}
