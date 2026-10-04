use crate::error::ErrorPayload;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProcessFailure {
    pub pid: u32,
    pub error: ErrorPayload,
}

/// Outcome of a stop request. `signaled` processes were asked to exit; they may
/// still be alive when this returns (no silent escalation to force kill).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StopReport {
    pub signaled: Vec<u32>,
    pub failed: Vec<ProcessFailure>,
}
