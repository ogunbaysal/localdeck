use super::process::ProcessOrigin;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    Web,
    Api,
    Worker,
    Database,
    Cache,
    Queue,
    Bundler,
    Watcher,
    Desktop,
    Cli,
    Runtime,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ComponentStatus {
    Running,
    /// At least one process of the component could only be partially inspected.
    Limited,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub cpu_percent: f32,
    #[cfg_attr(test, ts(type = "number"))]
    pub memory_bytes: u64,
    pub process_count: u32,
}

/// A logical command tree (for example `bun → sh → vite → esbuild`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: String,
    /// Stable key used to persist a user's kind correction.
    pub key: String,
    pub name: String,
    pub kind: ComponentKind,
    /// What the deterministic classifier chose before any user override.
    pub detected_kind: ComponentKind,
    pub kind_overridden: bool,
    pub technology: Option<String>,
    pub root_pid: u32,
    pub process_ids: Vec<u32>,
    pub ports: Vec<u16>,
    pub status: ComponentStatus,
    pub origin: ProcessOrigin,
    /// Short human summary of the root command (may contain secrets; never persisted).
    pub command: Option<String>,
    pub metrics: Metrics,
}
