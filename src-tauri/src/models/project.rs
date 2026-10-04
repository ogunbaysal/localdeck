use super::{ActivityEvent, Component, Metrics, ProcessInfo, ProjectId};
use serde::{Deserialize, Serialize};

/// Runtime state. "Attention" is orthogonal and lives in `Project::attention`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum RuntimeStatus {
    Running,
    /// Active, but a previously-known/configured primary runtime is absent.
    Partial,
    Stopped,
}

/// Which evidence selected the project root (explainability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum RootKind {
    Git,
    Workspace,
    Manifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AttentionKind {
    PortConflict,
    ManagedExited,
    RuntimeLost,
    ActionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub id: String,
    pub kind: AttentionKind,
    pub message: String,
    /// Transient items can be dismissed; derived ones clear themselves.
    pub dismissible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PackageManager {
    Bun,
    Pnpm,
    Yarn,
    Npm,
}

impl PackageManager {
    pub fn program(self) -> &'static str {
        match self {
            Self::Bun => "bun",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Npm => "npm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StackTag {
    /// Stable identifier (`nextjs`, `tauri`, …).
    pub id: String,
    pub label: String,
}

/// Normalised spawn representation: `program + args[] + cwd`, never a shell string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PrimaryCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// Display label such as `bun run dev`.
    pub label: String,
    /// `package.json` script this command was derived from.
    pub script: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PrimaryUrlSource {
    Override,
    WebComponent,
    DevServer,
    ConfirmedHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PrimaryUrl {
    pub url: String,
    pub source: PrimaryUrlSource,
}

/// User-editable overrides surfaced to the project settings dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectOverrides {
    pub display_name: Option<String>,
    pub primary_url: Option<String>,
    pub preferred_editor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[cfg_attr(test, ts(type = "number"))]
    pub id: ProjectId,
    /// Display name (override → detected, humanised).
    pub name: String,
    pub detected_name: String,
    pub root_path: String,
    pub root_kind: RootKind,
    pub runtime_status: RuntimeStatus,
    /// A graceful stop was requested and processes are still alive.
    pub stop_pending: bool,
    pub attention: Vec<AttentionItem>,
    pub stack: Vec<StackTag>,
    pub package_manager: Option<PackageManager>,
    pub components: Vec<Component>,
    /// Attributed processes in tree order (diagnostics).
    pub processes: Vec<ProcessInfo>,
    /// Sorted listening ports owned by the project.
    pub ports: Vec<u16>,
    pub primary_url: Option<PrimaryUrl>,
    pub primary_command: Option<PrimaryCommand>,
    pub overrides: ProjectOverrides,
    pub metrics: Metrics,
    #[cfg_attr(test, ts(type = "number"))]
    pub first_seen_at: i64,
    #[cfg_attr(test, ts(type = "number"))]
    pub last_seen_at: i64,
    pub favorite: bool,
    pub ignored: bool,
    pub recent_activity: Vec<ActivityEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PackageScript {
    pub name: String,
    pub command: String,
}

/// `package.json` scripts plus the package manager LocalDeck would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectScripts {
    pub package_manager: PackageManager,
    /// False when the manager is only the default because no lockfile exists.
    pub package_manager_detected: bool,
    pub scripts: Vec<PackageScript>,
    pub primary_script: Option<String>,
}
