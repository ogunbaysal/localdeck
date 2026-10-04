//! Pure command specs for the OS helpers LocalDeck launches (`open`, `xdg-open`, editors,
//! terminals) plus the small executor that runs them.
//!
//! Builders take an [`Os`] so both macOS and Linux behaviour is unit-testable on any host.
//! Nothing here ever goes through a shell: every value is a separate `argv` entry.

use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// How long a short-lived helper (`open`, `xdg-open`) may run before we stop watching it.
const HELPER_GRACE: Duration = Duration::from_secs(5);
const HELPER_POLL: Duration = Duration::from_millis(20);

/// Operating systems LocalDeck supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Os {
    MacOs,
    Linux,
}

impl Os {
    /// The OS this binary runs on; anything but macOS/Linux is unsupported.
    pub(crate) fn current() -> AppResult<Self> {
        if cfg!(target_os = "macos") {
            Ok(Self::MacOs)
        } else if cfg!(target_os = "linux") {
            Ok(Self::Linux)
        } else {
            Err(AppError::Unsupported(
                "LocalDeck supports macOS and Linux only".to_string(),
            ))
        }
    }
}

/// What to do with the process after spawning it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Launch {
    /// Short-lived helper that hands the work to the desktop and exits: wait briefly and report a
    /// failing exit status.
    Helper,
    /// Long-lived app or terminal: report spawn errors only and reap the process in the background.
    Detached,
}

/// A fully resolved process invocation (`program + args[] + cwd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub launch: Launch,
}

impl CommandSpec {
    pub(crate) fn new(program: impl Into<OsString>, launch: Launch) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            launch,
        }
    }

    pub(crate) fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub(crate) fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Spawns the command with null stdio in its own process group.
    pub(crate) fn run(&self) -> AppResult<()> {
        self.run_with_grace(HELPER_GRACE)
    }

    fn run_with_grace(&self, grace: Duration) -> AppResult<()> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        let name = Path::new(&self.program)
            .file_name()
            .unwrap_or(&self.program)
            .to_string_lossy();
        let child = command
            .spawn()
            .map_err(|err| AppError::OpenFailed(format!("Could not run `{name}`: {err}")))?;
        match self.launch {
            Launch::Detached => {
                reap_in_background(child);
                Ok(())
            }
            Launch::Helper => wait_for_helper(child, &name, grace),
        }
    }
}

fn wait_for_helper(mut child: Child, name: &str, grace: Duration) -> AppResult<()> {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(AppError::OpenFailed(format!("`{name}` failed ({status})")))
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(HELPER_POLL),
            Ok(None) => {
                reap_in_background(child);
                return Ok(());
            }
            Err(err) => {
                return Err(AppError::OpenFailed(format!(
                    "Could not wait for `{name}`: {err}"
                )))
            }
        }
    }
}

/// Collects the exit status so a finished launch never lingers as a zombie.
fn reap_in_background(mut child: Child) {
    // If the thread cannot be spawned the child merely stays a zombie until LocalDeck exits.
    let _ = thread::Builder::new()
        .name("launch-reaper".to_string())
        .spawn(move || {
            let _ = child.wait();
        });
}

/// `open <target>` on macOS, `xdg-open <target>` on Linux. `target` must be a validated URL or an
/// absolute path (so it can never be parsed as an option).
pub(crate) fn opener_command(os: Os, target: &OsStr) -> CommandSpec {
    let program = match os {
        Os::MacOs => "open",
        Os::Linux => "xdg-open",
    };
    CommandSpec::new(program, Launch::Helper).arg(target)
}

/// Accepts only `http`/`https` URLs (scheme case-insensitive) with a valid host and optional port.
pub(crate) fn validate_http_url(url: &str) -> AppResult<()> {
    let invalid = || {
        AppError::InvalidInput(format!(
            "Only http and https URLs with a host can be opened: {url}"
        ))
    };
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid());
    }
    let (scheme, rest) = url.split_once("://").ok_or_else(invalid)?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(invalid());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if is_valid_host_port(host_port) {
        Ok(())
    } else {
        Err(invalid())
    }
}

fn is_valid_host_port(host_port: &str) -> bool {
    let (host, port, bracketed) = match host_port.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((addr, tail)) => (addr, tail, true),
            None => return false,
        },
        None => {
            let (host, port) = host_port.split_at(host_port.find(':').unwrap_or(host_port.len()));
            (host, port, false)
        }
    };
    let port_ok = match port.strip_prefix(':') {
        None => port.is_empty(),
        Some(digits) => {
            !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && digits.parse::<u16>().is_ok()
        }
    };
    let host_ok = if bracketed {
        host.contains(':')
            && host
                .chars()
                .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'))
    } else {
        host.chars().any(char::is_alphanumeric)
            && host
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_'))
    };
    host_ok && port_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn accepts_http_and_https_urls() {
        for url in [
            "http://localhost:3000",
            "HTTPS://example.com/a?b=1",
            "http://127.0.0.1:5173/#/route",
            "https://user:pw@example.com:8443/path",
            "http://[::1]:8080/",
            "http://[2001:db8::1]/",
            "http://my_app.test",
            "http://bücher.example/straße",
            "HtTp://localhost",
        ] {
            assert!(validate_http_url(url).is_ok(), "{url} should be accepted");
        }
    }

    #[test]
    fn rejects_other_schemes_missing_hosts_and_garbage() {
        for url in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "mailto:me@example.com",
            "http://",
            "https:///path",
            "http://:3000",
            "http://?q=1",
            "http://@/",
            "http://localhost:",
            "http://localhost:99999",
            "http://localhost:+80",
            "http://a:b:c",
            "http://[::1",
            "http://[]/",
            "http://[zz]/",
            "http://exa mple.com",
            "http://example.com/\nfile:///x",
            "localhost:3000",
            "not a url",
            "://example.com",
            "-a http://example.com",
            "http//example.com",
        ] {
            let err = validate_http_url(url).expect_err(url);
            assert!(
                matches!(err, AppError::InvalidInput(_)),
                "{url} should be InvalidInput, got {err:?}"
            );
        }
    }

    #[test]
    fn opener_uses_open_on_macos_and_xdg_open_on_linux() {
        let target = os("/Users/me/Projekte mit Leerzeichen/café ☕");
        let mac = opener_command(Os::MacOs, &target);
        assert_eq!(mac.program, os("open"));
        assert_eq!(mac.args, vec![target.clone()]);
        assert_eq!(mac.cwd, None);
        assert_eq!(mac.launch, Launch::Helper);

        let linux = opener_command(Os::Linux, &target);
        assert_eq!(linux.program, os("xdg-open"));
        assert_eq!(linux.args, vec![target]);

        let url = opener_command(Os::Linux, OsStr::new("http://localhost:3000/?a=1&b=2"));
        assert_eq!(url.args, vec![os("http://localhost:3000/?a=1&b=2")]);
    }

    fn sh(script: &str, launch: Launch) -> CommandSpec {
        CommandSpec::new("/bin/sh", launch).arg("-c").arg(script)
    }

    #[test]
    fn helper_reports_success_and_failing_exit_status() {
        sh("exit 0", Launch::Helper).run().unwrap();
        let err = sh("exit 3", Launch::Helper).run().unwrap_err();
        assert!(
            matches!(&err, AppError::OpenFailed(m) if m.contains("`sh` failed") && m.contains('3')),
            "{err:?}"
        );
    }

    #[test]
    fn spawn_failure_is_open_failed() {
        let err = CommandSpec::new("/definitely/not/a/program", Launch::Helper)
            .run()
            .unwrap_err();
        assert!(
            matches!(&err, AppError::OpenFailed(m) if m.contains("`program`")),
            "{err:?}"
        );
        let err = CommandSpec::new("/definitely/not/a/program", Launch::Detached)
            .run()
            .unwrap_err();
        assert!(matches!(err, AppError::OpenFailed(_)));
    }

    #[test]
    fn detached_launch_does_not_wait_or_judge_exit_status() {
        let started = Instant::now();
        sh("sleep 1; exit 9", Launch::Detached).run().unwrap();
        assert!(started.elapsed() < Duration::from_millis(900));
    }

    #[test]
    fn helper_still_running_after_grace_is_handed_to_the_reaper() {
        let started = Instant::now();
        sh("sleep 1", Launch::Helper)
            .run_with_grace(Duration::from_millis(100))
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(900));
    }

    #[test]
    fn run_applies_cwd_with_spaces_and_non_ascii() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj ünï 日本");
        std::fs::create_dir(&dir).unwrap();
        sh("pwd > marker.txt", Launch::Helper)
            .cwd(&dir)
            .run()
            .unwrap();
        let recorded = std::fs::read_to_string(dir.join("marker.txt")).unwrap();
        assert_eq!(
            Path::new(recorded.trim_end()).canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
    }
}
