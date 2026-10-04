use super::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    ProjectStarted,
    ProjectStopped,
    PortOpened,
    PortClosed,
    PortConflict,
    ManagedExited,
    RuntimeLost,
}

impl ActivityKind {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::ProjectStarted => "project_started",
            Self::ProjectStopped => "project_stopped",
            Self::PortOpened => "port_opened",
            Self::PortClosed => "port_closed",
            Self::PortConflict => "port_conflict",
            Self::ManagedExited => "managed_exited",
            Self::RuntimeLost => "runtime_lost",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "project_started" => Self::ProjectStarted,
            "project_stopped" => Self::ProjectStopped,
            "port_opened" => Self::PortOpened,
            "port_closed" => Self::PortClosed,
            "port_conflict" => Self::PortConflict,
            "managed_exited" => Self::ManagedExited,
            "runtime_lost" => Self::RuntimeLost,
            _ => return None,
        })
    }
}

/// Small, non-sensitive facts about an event. Never raw command lines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActivityMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub exit_code: Option<i32>,
    /// The project was already running when LocalDeck first looked.
    #[serde(default)]
    pub detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    #[cfg_attr(test, ts(type = "number"))]
    pub id: i64,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub project_id: Option<ProjectId>,
    pub kind: ActivityKind,
    pub metadata: ActivityMetadata,
    /// Human-readable description built from kind + metadata.
    pub message: String,
    /// Epoch milliseconds.
    #[cfg_attr(test, ts(type = "number"))]
    pub created_at: i64,
}

impl ActivityEvent {
    pub fn describe(kind: ActivityKind, meta: &ActivityMetadata) -> String {
        let component = meta.component.as_deref();
        match kind {
            ActivityKind::ProjectStarted if meta.detected => "Detected running".to_string(),
            ActivityKind::ProjectStarted => "Project started".to_string(),
            ActivityKind::ProjectStopped => "Project stopped".to_string(),
            ActivityKind::PortOpened => match meta.port {
                Some(port) => format!("Port {port} opened"),
                None => "Port opened".to_string(),
            },
            ActivityKind::PortClosed => match meta.port {
                Some(port) => format!("Port {port} closed"),
                None => "Port closed".to_string(),
            },
            ActivityKind::PortConflict => match meta.port {
                Some(port) => format!("Port {port} has ambiguous ownership"),
                None => "Port ownership is ambiguous".to_string(),
            },
            ActivityKind::ManagedExited => match (component, meta.exit_code) {
                (_, Some(code)) => format!("Managed command exited with code {code}"),
                _ => "Managed command exited".to_string(),
            },
            ActivityKind::RuntimeLost => match component {
                Some(name) => format!("{name} disappeared while the project kept running"),
                None => "A component disappeared while the project kept running".to_string(),
            },
        }
    }
}
