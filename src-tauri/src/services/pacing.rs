//! Adaptive scan pacing. A scan costs real CPU and wakes the machine, so the
//! cadence follows what can actually be seen:
//!
//! * **Window** — the window is visible: live UI, fastest cadence.
//! * **Tray** — the user just interacted with the tray icon/menu: a medium
//!   cadence for a short boost period, so the menu they are about to open is
//!   fresh.
//! * **Background** — only the tray exists: slowest cadence. Crash detection,
//!   notifications and activity still work, just with a longer delay.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const WINDOW_INTERVAL: Duration = Duration::from_secs(2);
pub const TRAY_INTERVAL: Duration = Duration::from_secs(4);
pub const BACKGROUND_INTERVAL: Duration = Duration::from_secs(10);
/// How long a tray interaction keeps the medium cadence.
pub const TRAY_BOOST: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Window,
    Tray,
    Background,
}

impl Mode {
    pub fn interval(self) -> Duration {
        match self {
            Self::Window => WINDOW_INTERVAL,
            Self::Tray => TRAY_INTERVAL,
            Self::Background => BACKGROUND_INTERVAL,
        }
    }
}

pub struct Pacing {
    window_active: AtomicBool,
    tray_until: Mutex<Option<Instant>>,
    current_ms: AtomicU32,
}

impl Default for Pacing {
    fn default() -> Self {
        Self {
            // Until told otherwise assume the window is up (first launch).
            window_active: AtomicBool::new(true),
            tray_until: Mutex::new(None),
            current_ms: AtomicU32::new(WINDOW_INTERVAL.as_millis() as u32),
        }
    }
}

impl Pacing {
    /// Window visible and not minimized.
    pub fn set_window_active(&self, active: bool) {
        self.window_active.store(active, Ordering::Relaxed);
    }

    pub fn window_active(&self) -> bool {
        self.window_active.load(Ordering::Relaxed)
    }

    /// The user hovered/clicked the tray icon.
    pub fn tray_interacted(&self, now: Instant) {
        *self.tray_until.lock().unwrap_or_else(|p| p.into_inner()) = Some(now + TRAY_BOOST);
    }

    pub fn mode(&self, now: Instant) -> Mode {
        if self.window_active() {
            return Mode::Window;
        }
        let until = *self.tray_until.lock().unwrap_or_else(|p| p.into_inner());
        if until.is_some_and(|u| now < u) {
            Mode::Tray
        } else {
            Mode::Background
        }
    }

    /// How long the scanner may sleep: the mode's interval, shortened so a due
    /// auto-restart is not delayed by a slow cadence.
    pub fn wait(
        &self,
        now: Instant,
        since_last_scan: Duration,
        restart_due: Option<Instant>,
    ) -> Duration {
        let interval = self.mode(now).interval();
        self.current_ms.store(
            interval.as_millis().min(u128::from(u32::MAX)) as u32,
            Ordering::Relaxed,
        );
        let mut wait = interval.saturating_sub(since_last_scan);
        if let Some(due) = restart_due {
            wait = wait.min(due.saturating_duration_since(now));
        }
        wait
    }

    /// Interval currently in effect, reported in the scanner status.
    pub fn current_ms(&self) -> u32 {
        self.current_ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_cadences_are_distinct_and_ordered() {
        assert!(WINDOW_INTERVAL < TRAY_INTERVAL && TRAY_INTERVAL < BACKGROUND_INTERVAL);
    }

    #[test]
    fn window_beats_tray_beats_background() {
        let p = Pacing::default();
        let now = Instant::now();
        assert_eq!(p.mode(now), Mode::Window);
        p.set_window_active(false);
        assert_eq!(p.mode(now), Mode::Background);
        p.tray_interacted(now);
        assert_eq!(p.mode(now), Mode::Tray);
        p.set_window_active(true);
        assert_eq!(p.mode(now), Mode::Window, "a visible window always wins");
    }

    #[test]
    fn the_tray_boost_expires_and_can_be_renewed() {
        let p = Pacing::default();
        p.set_window_active(false);
        let t0 = Instant::now();
        p.tray_interacted(t0);
        assert_eq!(
            p.mode(t0 + TRAY_BOOST - Duration::from_millis(1)),
            Mode::Tray
        );
        assert_eq!(p.mode(t0 + TRAY_BOOST), Mode::Background);
        p.tray_interacted(t0 + TRAY_BOOST);
        assert_eq!(p.mode(t0 + TRAY_BOOST + Duration::from_secs(1)), Mode::Tray);
    }

    #[test]
    fn wait_counts_down_from_the_last_scan_and_reports_the_current_interval() {
        let p = Pacing::default();
        p.set_window_active(false);
        let now = Instant::now();
        assert_eq!(
            p.wait(now, Duration::from_secs(3), None),
            Duration::from_secs(7)
        );
        assert_eq!(p.current_ms(), 10_000);
        assert_eq!(p.wait(now, Duration::from_secs(30), None), Duration::ZERO);
        p.set_window_active(true);
        assert_eq!(p.wait(now, Duration::ZERO, None), WINDOW_INTERVAL);
        assert_eq!(p.current_ms(), 2_000);
    }

    #[test]
    fn a_due_auto_restart_is_never_delayed_by_the_slow_cadence() {
        let p = Pacing::default();
        p.set_window_active(false);
        let now = Instant::now();
        let due = now + Duration::from_secs(2);
        assert_eq!(
            p.wait(now, Duration::ZERO, Some(due)),
            Duration::from_secs(2)
        );
        assert_eq!(
            p.wait(now, Duration::ZERO, Some(now - Duration::from_secs(1))),
            Duration::ZERO
        );
        assert_eq!(
            p.wait(now, Duration::ZERO, Some(now + Duration::from_secs(60))),
            BACKGROUND_INTERVAL
        );
    }
}
