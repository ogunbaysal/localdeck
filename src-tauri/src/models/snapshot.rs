use super::{PortInfo, Project};
use crate::error::ErrorPayload;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Macos,
    Linux,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AccessSummary {
    pub total: u32,
    pub full: u32,
    pub partial: u32,
    pub unavailable: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScannerStatus {
    pub interval_ms: u32,
    #[cfg_attr(test, ts(type = "number"))]
    pub scan_count: u64,
    pub last_scan_ms: u32,
    pub process_access: AccessSummary,
    /// False when the socket table could not be read at all.
    pub sockets_available: bool,
    pub socket_error: Option<ErrorPayload>,
    /// Sockets/PIDs the OS refused to describe during the last scan.
    pub sockets_skipped: u32,
    pub persistence_error: Option<ErrorPayload>,
    pub platform: Platform,
}

/// Everything the UI renders. Rust-owned; React keeps a projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub projects: Vec<Project>,
    pub ports: Vec<PortInfo>,
    pub scanner: ScannerStatus,
    /// Epoch milliseconds of the scan that produced this snapshot.
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: i64,
}
