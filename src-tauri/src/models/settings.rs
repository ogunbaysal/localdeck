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
    /// Terminal id from the platform catalogue (`None` = platform default).
    pub preferred_terminal: Option<String>,
    /// Native notifications for crashes, conflicts and lost components.
    pub notifications_enabled: bool,
    /// Accelerator such as `CommandOrControl+Shift+K` that summons the palette.
    pub global_shortcut: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            launch_at_login: false,
            preferred_editor: None,
            forget_after_days: None,
            preferred_terminal: None,
            notifications_enabled: true,
            global_shortcut: None,
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

/// A terminal application LocalDeck knows how to open at a project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
}
