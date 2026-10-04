use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub launch_at_login: bool,
    /// Editor id from the built-in catalogue (for example `vscode`).
    pub preferred_editor: Option<String>,
    /// Forget stopped projects not seen for this many days. `None` keeps them forever.
    pub forget_after_days: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            launch_at_login: false,
            preferred_editor: None,
            forget_after_days: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EditorInfo {
    pub id: String,
    pub name: String,
    /// Detected on this machine.
    pub available: bool,
}
