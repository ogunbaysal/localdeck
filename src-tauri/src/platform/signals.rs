//! Sending signals to a single process.

use std::io;

use super::Signal;
use crate::error::{AppError, AppResult};

/// Sends `signal` to exactly one process. Pids 0 and 1 and anything above `i32::MAX` are refused
/// so a bad value can never address a process group (`kill(0)`, negative pids) or init.
pub fn send_signal(pid: u32, signal: Signal) -> AppResult<()> {
    let target = match libc::pid_t::try_from(pid) {
        Ok(target) if target > 1 => target,
        _ => {
            return Err(AppError::InvalidInput(format!(
                "Refusing to signal process id {pid}"
            )))
        }
    };
    let raw = match signal {
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };
    // SAFETY: `kill(2)` takes plain integers; `target > 1` addresses a single process only.
    if unsafe { libc::kill(target, raw) } == 0 {
        return Ok(());
    }
    Err(kill_error(pid, io::Error::last_os_error().raw_os_error()))
}

fn kill_error(pid: u32, errno: Option<i32>) -> AppError {
    match errno {
        Some(libc::ESRCH) => AppError::ProcessNotFound { pid },
        Some(libc::EPERM) => AppError::ProcessPermissionDenied { pid },
        other => AppError::InvalidInput(format!(
            "Could not signal process {pid} (errno {})",
            other.map_or_else(|| "unknown".to_string(), |code| code.to_string())
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command, Stdio};

    fn spawn_sleeper() -> Child {
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn terminate_stops_the_process_with_sigterm() {
        let mut child = spawn_sleeper();
        send_signal(child.id(), Signal::Terminate).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn kill_stops_the_process_with_sigkill() {
        let mut child = spawn_sleeper();
        send_signal(child.id(), Signal::Kill).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    #[test]
    fn signalling_a_reaped_process_is_process_not_found() {
        let mut child = spawn_sleeper();
        let pid = child.id();
        child.kill().unwrap();
        child.wait().unwrap();
        for signal in [Signal::Terminate, Signal::Kill] {
            let err = send_signal(pid, signal).unwrap_err();
            assert!(
                matches!(err, AppError::ProcessNotFound { pid: p } if p == pid),
                "{err:?}"
            );
        }
    }

    #[test]
    fn refuses_pids_that_could_address_groups_or_init() {
        for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX] {
            let err = send_signal(pid, Signal::Terminate).unwrap_err();
            assert!(
                matches!(err, AppError::InvalidInput(_)),
                "pid {pid}: {err:?}"
            );
        }
    }

    #[test]
    fn errno_mapping_distinguishes_missing_denied_and_other() {
        assert!(matches!(
            kill_error(7, Some(libc::ESRCH)),
            AppError::ProcessNotFound { pid: 7 }
        ));
        assert!(matches!(
            kill_error(7, Some(libc::EPERM)),
            AppError::ProcessPermissionDenied { pid: 7 }
        ));
        assert!(matches!(
            kill_error(7, Some(libc::EINVAL)),
            AppError::InvalidInput(_)
        ));
        assert!(matches!(kill_error(7, None), AppError::InvalidInput(_)));
    }
}
