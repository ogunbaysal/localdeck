use super::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Tcp,
}

/// How reliably a listening endpoint maps to a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PortOwnership {
    /// One project, one component.
    Assigned,
    /// One project, but several of its components hold the endpoint.
    Shared,
    /// Several PIDs that point at different projects (or project + unknown).
    Ambiguous,
    /// No PID/cwd/process-tree evidence ties it to a project.
    Unassigned,
}

/// HTTP evidence for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HttpState {
    /// A local HTTP probe received a valid HTTP response.
    Confirmed,
    /// A known dev-server process owns it (no probe needed).
    Likely,
    /// A probe is in flight.
    Probing,
    /// Not (confirmed to be) HTTP.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PortProcess {
    pub pid: u32,
    pub name: Option<String>,
    /// Process start time (epoch ms); lets terminate/kill detect PID reuse.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub started_at: Option<i64>,
}

/// One listening TCP endpoint (merged across wildcard IPv4/IPv6 twins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub id: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub transport: Transport,
    /// Every PID the OS reported. Never collapsed to a guess.
    pub processes: Vec<PortProcess>,
    /// False when the OS reported the socket but not its owner (permissions).
    pub owner_visible: bool,
    pub ownership: PortOwnership,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub project_id: Option<ProjectId>,
    pub component_id: Option<String>,
    pub http: HttpState,
    pub is_http: bool,
    /// Browser-usable URL for HTTP endpoints.
    pub url: Option<String>,
}
