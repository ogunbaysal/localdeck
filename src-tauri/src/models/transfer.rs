use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportSkip {
    /// Project root path (or `settings`) the entry referred to.
    pub subject: String,
    pub reason: String,
}

/// Outcome of importing an overrides file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub applied: u32,
    pub settings_applied: bool,
    pub skipped: Vec<ImportSkip>,
}
