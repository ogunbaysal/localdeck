//! Background scanner: one dedicated thread, one scan at a time (overlap is
//! impossible by construction), ~2 s cadence, early wake-up after actions.

use super::runtime::{Runtime, ScanResult};
use crate::engine::providers::{NetstatSocketProvider, SysinfoProcessProvider};
use crate::engine::raw::{ProcessProvider, SocketProvider};
use crate::error::{AppError, AppResult};
use crate::models::AppSnapshot;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Minimum spacing between scans so CPU deltas stay meaningful.
const MIN_SCAN_SPACING: Duration = Duration::from_millis(500);

pub struct Scanner {
    processes: SysinfoProcessProvider,
    sockets: NetstatSocketProvider,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            processes: SysinfoProcessProvider::new(),
            sockets: NetstatSocketProvider,
        }
    }

    /// One full scan applied to `runtime`.
    pub fn scan_once(&mut self, runtime: &Runtime) -> Arc<AppSnapshot> {
        runtime.run_due_restarts(Instant::now());
        let started = Instant::now();
        let processes = self.processes.snapshot().unwrap_or_else(|err| {
            tracing::warn!(%err, "process listing failed");
            Vec::new()
        });
        let sockets = self.sockets.listening_sockets();
        let duration_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
        runtime.apply_scan(ScanResult {
            processes,
            sockets,
            duration_ms,
        })
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns the scanner thread. `on_snapshot` runs after every scan (emit event,
/// refresh tray). The first scan is expected to have been done by the caller.
pub fn spawn_scanner<F>(
    runtime: Arc<Runtime>,
    mut scanner: Scanner,
    on_snapshot: F,
) -> AppResult<()>
where
    F: Fn(Arc<AppSnapshot>) + Send + 'static,
{
    let (trigger, wake) = mpsc::channel::<()>();
    runtime.set_scan_trigger(trigger);
    std::thread::Builder::new()
        .name("localdeck-scanner".to_string())
        .spawn(move || {
            let mut last = Instant::now();
            loop {
                // The cadence follows what is visible (window / tray / neither).
                let wait =
                    runtime
                        .pacing
                        .wait(Instant::now(), last.elapsed(), runtime.next_restart_due());
                match wake.recv_timeout(wait) {
                    Ok(()) => {
                        while wake.try_recv().is_ok() {}
                        let since = last.elapsed();
                        if since < MIN_SCAN_SPACING {
                            std::thread::sleep(MIN_SCAN_SPACING - since);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                last = Instant::now();
                match catch_unwind(AssertUnwindSafe(|| scanner.scan_once(&runtime))) {
                    Ok(snapshot) => on_snapshot(snapshot),
                    Err(_) => tracing::error!("scan panicked; continuing with the next tick"),
                }
            }
        })
        .map_err(|e| AppError::PortScanFailed(format!("could not start scanner thread: {e}")))?;
    Ok(())
}
