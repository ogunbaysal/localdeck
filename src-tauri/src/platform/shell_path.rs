//! The user's real `PATH`.
//!
//! A GUI-launched macOS app inherits a minimal `PATH` (no Homebrew, bun, cargo…). The first call
//! to [`user_path`] asks the user's login shell for its `PATH` once and caches it for the process
//! lifetime. That call can block for up to [`SHELL_TIMEOUT`], so use it from blocking contexts.

use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, LazyLock};
use std::thread;
use std::time::Duration;

const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on shell output we are willing to buffer while waiting for the end marker.
const MAX_PROBE_OUTPUT: usize = 1 << 20;

const BEGIN_MARKER: &str = "__LOCALDECK_PATH_BEGIN__";
const END_MARKER: &str = "__LOCALDECK_PATH_END__";
/// Fixed script (no user input) run by the login shell. The markers let us ignore whatever the
/// user's rc files print.
const PROBE_SCRIPT: &str = "printf '%s\\n' __LOCALDECK_PATH_BEGIN__; command printenv PATH; printf '%s\\n' __LOCALDECK_PATH_END__";

/// The `PATH` a terminal-launched command would see, computed once.
pub fn user_path() -> &'static OsStr {
    static USER_PATH: LazyLock<OsString> = LazyLock::new(compute_user_path);
    USER_PATH.as_os_str()
}

fn compute_user_path() -> OsString {
    let current = env::var_os("PATH").unwrap_or_default();
    let home = home_dir();
    let shell = login_shell_path();
    if shell.is_none() {
        tracing::warn!("could not read PATH from the login shell; using fallback directories");
    }
    combine_paths(
        shell.as_deref(),
        &current,
        &join_dirs(&fallback_dirs(home.as_deref())),
    )
}

/// Shell PATH first, then the process PATH. Fallback directories only matter when the shell
/// could not be asked.
fn combine_paths(shell: Option<&OsStr>, current: &OsStr, fallback: &OsStr) -> OsString {
    match shell {
        Some(shell) => merge_paths(&[shell, current]),
        None => merge_paths(&[current, fallback]),
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
}

fn fallback_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home {
        dirs.extend([".bun/bin", ".cargo/bin", ".local/bin"].map(|dir| home.join(dir)));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/snap/bin"].map(PathBuf::from));
    dirs
}

pub(crate) fn join_dirs(dirs: &[PathBuf]) -> OsString {
    // `join_paths` only fails for entries containing the separator, which our callers never pass.
    env::join_paths(dirs).unwrap_or_default()
}

/// Concatenates `PATH`-style lists, dropping empty and relative entries (they would resolve
/// against LocalDeck's own working directory) and duplicates. First occurrence wins.
pub(crate) fn merge_paths(parts: &[&OsStr]) -> OsString {
    let mut merged: Vec<PathBuf> = Vec::new();
    for dir in parts.iter().flat_map(|part| env::split_paths(part)) {
        if dir.is_absolute() && !merged.contains(&dir) {
            merged.push(dir);
        }
    }
    env::join_paths(merged).unwrap_or_default()
}

/// Extracts the PATH printed between the probe markers, ignoring any rc-file noise around them.
pub(crate) fn parse_path_output(stdout: &str) -> Option<String> {
    let after_begin = stdout.split_once(BEGIN_MARKER)?.1;
    let between = after_begin.split_once(END_MARKER)?.0;
    between
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Finds a bare command name (no `/`) in `path_var`, like a shell would: first match that is an
/// executable file wins. Empty and relative entries are skipped.
pub(crate) fn resolve_program(program: &str, path_var: &OsStr) -> Option<PathBuf> {
    if program.is_empty() || program.contains('/') {
        return None;
    }
    env::split_paths(path_var)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

/// Regular file (following symlinks) the current user may execute.
pub(crate) fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

fn login_shell_path() -> Option<OsString> {
    let shell = env::var_os("SHELL")
        .filter(|shell| Path::new(shell).is_absolute())
        .unwrap_or_else(|| OsString::from("/bin/sh"));
    probe_path(
        &shell,
        &[OsStr::new("-ilc"), OsStr::new(PROBE_SCRIPT)],
        SHELL_TIMEOUT,
    )
}

/// Runs `program args…` with null stdin and returns the PATH it prints between the markers.
/// Kills the whole session if it does not finish within `timeout`.
fn probe_path(program: &OsStr, args: &[&OsStr], timeout: Duration) -> Option<OsString> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: the closure only calls `setsid`, which is async-signal-safe. A fresh session means
    // no controlling terminal, so an interactive shell cannot stop on tty access (the same
    // approach VS Code uses to resolve the shell environment).
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read_until_end_marker(stdout));
    });
    let output = receiver.recv_timeout(timeout).ok();
    // The shell has nothing left to do. Taking down its session (it is the group leader) also
    // stops rc-file leftovers that could keep the stdout pipe open.
    if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: `pgid` is the freshly spawned, not yet reaped child, so it cannot have been
        // recycled; it is always > 1.
        unsafe { libc::killpg(pgid, libc::SIGKILL) };
    }
    let _ = child.wait();
    let text = String::from_utf8_lossy(&output?).into_owned();
    parse_path_output(&text).map(OsString::from)
}

/// Reads until EOF, an error, the size cap, or the end marker is seen.
fn read_until_end_marker(mut reader: impl Read) -> Vec<u8> {
    let marker = END_MARKER.as_bytes();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    while buffer.len() < MAX_PROBE_OUTPUT {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                // Rescan a marker-sized overlap so a marker split across reads is found.
                let scan_from = buffer.len().saturating_sub(marker.len());
                buffer.extend_from_slice(&chunk[..read]);
                if buffer[scan_from..]
                    .windows(marker.len())
                    .any(|window| window == marker)
                {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::test_util::{executable, plain_file};
    use std::time::Instant;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn parse_ignores_rc_noise_around_the_markers() {
        let stdout = format!(
            "Welcome back!\nPATH=/wrong\n{BEGIN_MARKER}\n/opt/homebrew/bin:/usr/bin\n{END_MARKER}\nbye\n"
        );
        assert_eq!(
            parse_path_output(&stdout).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
    }

    #[test]
    fn parse_handles_crlf_and_blank_lines_between_markers() {
        let stdout = format!("{BEGIN_MARKER}\r\n\r\n /a:/b \r\n\r\n{END_MARKER}\r\n");
        assert_eq!(parse_path_output(&stdout).as_deref(), Some("/a:/b"));
    }

    #[test]
    fn parse_rejects_missing_markers_and_empty_values() {
        assert_eq!(parse_path_output(""), None);
        assert_eq!(parse_path_output("/usr/bin:/bin\n"), None);
        assert_eq!(
            parse_path_output(&format!("{BEGIN_MARKER}\n/usr/bin\n")),
            None
        );
        assert_eq!(
            parse_path_output(&format!("{END_MARKER}\n{BEGIN_MARKER}\n")),
            None
        );
        assert_eq!(
            parse_path_output(&format!("{BEGIN_MARKER}\n\n  \n{END_MARKER}\n")),
            None
        );
    }

    #[test]
    fn merge_dedupes_keeps_order_and_first_list_wins() {
        let merged = merge_paths(&[
            OsStr::new("/opt/homebrew/bin:/usr/bin:/opt/homebrew/bin"),
            OsStr::new("/usr/bin:/bin:/opt/homebrew/bin/:/usr/local/bin"),
        ]);
        assert_eq!(merged, os("/opt/homebrew/bin:/usr/bin:/bin:/usr/local/bin"));
    }

    #[test]
    fn merge_drops_empty_and_relative_entries() {
        let merged = merge_paths(&[OsStr::new(":.:bin:/usr/bin::../x"), OsStr::new("")]);
        assert_eq!(merged, os("/usr/bin"));
        assert_eq!(merge_paths(&[]), os(""));
    }

    #[test]
    fn merge_preserves_non_ascii_and_spaces() {
        let merged = merge_paths(&[
            OsStr::new("/Users/zoë/My Tools/bin"),
            OsStr::new("/Users/zoë/My Tools/bin:/ü/日本"),
        ]);
        assert_eq!(merged, os("/Users/zoë/My Tools/bin:/ü/日本"));
    }

    #[test]
    fn combine_prefers_shell_and_only_uses_fallback_without_it() {
        let current = OsStr::new("/usr/bin:/bin");
        let fallback = OsStr::new("/opt/homebrew/bin:/usr/bin");
        assert_eq!(
            combine_paths(
                Some(OsStr::new("/opt/homebrew/bin:/bin")),
                current,
                fallback
            ),
            os("/opt/homebrew/bin:/bin:/usr/bin")
        );
        assert_eq!(
            combine_paths(None, current, fallback),
            os("/usr/bin:/bin:/opt/homebrew/bin")
        );
    }

    #[test]
    fn fallback_dirs_cover_well_known_tool_locations() {
        let dirs = fallback_dirs(Some(Path::new("/home/zoë x")));
        assert_eq!(
            dirs,
            [
                "/home/zoë x/.bun/bin",
                "/home/zoë x/.cargo/bin",
                "/home/zoë x/.local/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/snap/bin"
            ]
            .map(PathBuf::from)
        );
        assert_eq!(fallback_dirs(None).len(), 3);
    }

    #[test]
    fn resolve_finds_first_executable_in_path_order() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("bin ünï 1");
        let second = tmp.path().join("bin 日本 2");
        let expected = executable(&second, "tool");
        plain_file(&first, "tool"); // not executable: skipped like a shell would
        let path_var = env::join_paths([&first, &second]).unwrap();
        assert_eq!(resolve_program("tool", &path_var), Some(expected));

        let winner = executable(&first, "other");
        executable(&second, "other");
        assert_eq!(resolve_program("other", &path_var), Some(winner));
    }

    #[test]
    fn resolve_rejects_non_executables_directories_and_non_bare_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my bin");
        plain_file(&dir, "readme");
        std::fs::create_dir(dir.join("subdir-tool")).unwrap();
        executable(&dir, "real");
        let path_var = env::join_paths([&dir]).unwrap();

        assert_eq!(resolve_program("readme", &path_var), None);
        assert_eq!(resolve_program("subdir-tool", &path_var), None);
        assert_eq!(resolve_program("missing", &path_var), None);
        assert_eq!(resolve_program("", &path_var), None);
        assert_eq!(resolve_program("my bin/real", tmp.path().as_os_str()), None);
        assert!(resolve_program("real", &path_var).is_some());
    }

    #[test]
    fn resolve_ignores_relative_path_entries() {
        // Resolves to `/bin` from any working directory, so only a relative-entry bug finds `sh`.
        let relative = "../../../../../../../../../../../../bin";
        assert!(Path::new("/bin/sh").exists());
        assert_eq!(resolve_program("sh", OsStr::new(relative)), None);
        assert!(resolve_program("sh", OsStr::new("/bin")).is_some());
    }

    fn sh_probe(script: &str, timeout: Duration) -> Option<OsString> {
        probe_path(
            OsStr::new("/bin/sh"),
            &[OsStr::new("-c"), OsStr::new(script)],
            timeout,
        )
    }

    #[test]
    fn probe_script_prints_the_environment_path_through_a_real_shell() {
        let expected = env::var_os("PATH").unwrap();
        let script = format!("echo noise; echo PATH=/evil; {PROBE_SCRIPT}; echo trailing");
        assert_eq!(sh_probe(&script, Duration::from_secs(10)), Some(expected));
        assert!(PROBE_SCRIPT.contains(BEGIN_MARKER) && PROBE_SCRIPT.contains(END_MARKER));
    }

    #[test]
    fn probe_survives_a_shell_that_exits_non_zero_after_printing() {
        let script = format!("{PROBE_SCRIPT}; exit 3");
        assert_eq!(
            sh_probe(&script, Duration::from_secs(10)),
            Some(env::var_os("PATH").unwrap())
        );
    }

    #[test]
    fn probe_returns_none_without_markers() {
        assert_eq!(sh_probe("exit 1", Duration::from_secs(10)), None);
        assert_eq!(sh_probe("echo /usr/bin", Duration::from_secs(10)), None);
        assert_eq!(
            probe_path(
                OsStr::new("/definitely/not/a/shell"),
                &[],
                Duration::from_secs(1)
            ),
            None
        );
    }

    #[test]
    fn probe_kills_a_hanging_shell_after_the_timeout() {
        let started = Instant::now();
        assert_eq!(sh_probe("sleep 30", Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_does_not_wait_for_background_jobs_holding_stdout() {
        let started = Instant::now();
        let script = format!("sleep 30 & {PROBE_SCRIPT}");
        assert!(sh_probe(&script, Duration::from_secs(10)).is_some());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.0.len().min(buf.len()).min(3);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn reader_finds_a_split_end_marker_and_stops_early() {
        let data = format!("{BEGIN_MARKER}\n/a:/b\n{END_MARKER}\nlots of trailing rc output");
        let read = read_until_end_marker(Trickle(data.as_bytes()));
        let text = String::from_utf8(read).unwrap();
        assert!(text.contains(END_MARKER));
        assert!(
            !text.contains("trailing"),
            "kept reading after the marker: {text:?}"
        );
    }

    #[test]
    fn reader_stops_at_eof_without_marker() {
        let read = read_until_end_marker(Trickle(b"just noise"));
        assert_eq!(read, b"just noise");
    }
}
