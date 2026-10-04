//! Serialisable runtime/domain models shared with the React frontend.
//!
//! These are LocalDeck's own types. Third-party structs (sysinfo, netstat2)
//! never leak past the providers in `engine::providers`.

mod action;
mod activity;
mod component;
mod port;
mod process;
mod project;
mod settings;
mod snapshot;

pub use action::{ProcessFailure, StopReport};
pub use activity::{ActivityEvent, ActivityKind, ActivityMetadata};
pub use component::{Component, ComponentKind, ComponentStatus, Metrics};
pub use port::{HttpState, PortInfo, PortOwnership, PortProcess, Transport};
pub use process::{AccessLevel, ProcessInfo, ProcessOrigin};
pub use project::{
    AttentionItem, AttentionKind, PackageManager, PackageScript, PrimaryCommand, PrimaryUrl,
    PrimaryUrlSource, Project, ProjectOverrides, ProjectScripts, RootKind, RuntimeStatus, StackTag,
};
pub use settings::{EditorInfo, Settings, Theme};
pub use snapshot::{AccessSummary, AppSnapshot, Platform, ScannerStatus};

/// Database identity of a project (stable across restarts).
pub type ProjectId = i64;
