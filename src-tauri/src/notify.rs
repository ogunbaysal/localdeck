//! Native notifications for crashes, conflicts and lost components. Delivery is
//! best effort: a failure here must never affect scanning.

use crate::services::runtime::UserNotification;
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// Where notifications go. A seam so dispatch can be tested without a desktop.
pub trait Sink {
    /// Ask for permission if needed; `false` means do not show anything.
    fn allowed(&self) -> bool;
    fn show(&self, title: &str, body: &str) -> Result<(), String>;
}

struct TauriSink<'a, R: Runtime>(&'a AppHandle<R>);

impl<R: Runtime> Sink for TauriSink<'_, R> {
    fn allowed(&self) -> bool {
        let notification = self.0.notification();
        match notification.permission_state() {
            Ok(PermissionState::Granted) => true,
            Ok(PermissionState::Denied) | Err(_) => false,
            Ok(_) => matches!(
                notification.request_permission(),
                Ok(PermissionState::Granted)
            ),
        }
    }

    fn show(&self, title: &str, body: &str) -> Result<(), String> {
        self.0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}

/// Deliver `notes` through `sink`; returns how many were shown.
pub fn dispatch(sink: &dyn Sink, notes: &[UserNotification]) -> usize {
    if notes.is_empty() || !sink.allowed() {
        return 0;
    }
    notes
        .iter()
        .filter(|note| match sink.show(&note.title, &note.body) {
            Ok(()) => true,
            Err(err) => {
                tracing::debug!(%err, "notification failed");
                false
            }
        })
        .count()
}

/// Show everything the runtime queued during the last scan.
pub fn deliver<R: Runtime>(app: &AppHandle<R>, notes: &[UserNotification]) {
    dispatch(&TauriSink(app), notes);
}

/// One immediate message (tray action failures).
pub fn show<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    let note = UserNotification {
        project_id: 0,
        title: title.to_string(),
        body: body.to_string(),
    };
    deliver(app, &[note]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        allowed: bool,
        fail_on: Option<&'static str>,
        shown: RefCell<Vec<(String, String)>>,
    }

    impl Sink for Fake {
        fn allowed(&self) -> bool {
            self.allowed
        }
        fn show(&self, title: &str, body: &str) -> Result<(), String> {
            if self.fail_on == Some(title) {
                return Err("boom".into());
            }
            self.shown.borrow_mut().push((title.into(), body.into()));
            Ok(())
        }
    }

    fn note(title: &str) -> UserNotification {
        UserNotification {
            project_id: 1,
            title: title.into(),
            body: format!("{title} crashed"),
        }
    }

    #[test]
    fn shows_everything_when_allowed_and_keeps_going_after_a_failure() {
        let sink = Fake {
            allowed: true,
            fail_on: Some("b"),
            shown: RefCell::default(),
        };
        assert_eq!(dispatch(&sink, &[note("a"), note("b"), note("c")]), 2);
        let shown = sink.shown.borrow();
        assert_eq!(
            shown.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
            ["a", "c"]
        );
    }

    #[test]
    fn permission_denied_or_nothing_to_say_shows_nothing() {
        let denied = Fake {
            allowed: false,
            fail_on: None,
            shown: RefCell::default(),
        };
        assert_eq!(dispatch(&denied, &[note("a")]), 0);
        assert!(denied.shown.borrow().is_empty());
        let ok = Fake {
            allowed: true,
            fail_on: None,
            shown: RefCell::default(),
        };
        assert_eq!(dispatch(&ok, &[]), 0);
    }
}
