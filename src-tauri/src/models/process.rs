use serde::{Deserialize, Serialize};

/// How much metadata the OS let us read for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    /// Executable, arguments and working directory are all known.
    Full,
    /// Some of them are missing (typically another user's process).
    Partial,
    /// Nothing beyond pid/name could be read.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ProcessOrigin {
    External,
    LocalDeckManaged,
}

/// A process attributed to a project (diagnostic detail, not a primary entity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Percent of one core (can exceed 100 for multi-threaded processes).
    pub cpu_usage: f32,
    #[cfg_attr(test, ts(type = "number"))]
    pub memory_bytes: u64,
    /// Process start time, epoch milliseconds.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub started_at: Option<i64>,
    pub access_level: AccessLevel,
    pub origin: ProcessOrigin,
    pub component_id: Option<String>,
    /// Depth inside the project's process forest (0 = component root).
    pub depth: u32,
}
