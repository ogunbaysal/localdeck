//! Global shortcut that summons the command palette.

use crate::error::{AppError, AppResult};
use crate::OPEN_PALETTE_EVENT;
use std::str::FromStr;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const MODIFIERS: &[(&str, &str)] = &[
    ("commandorcontrol", "CommandOrControl"),
    ("cmdorctrl", "CommandOrControl"),
    ("control", "Control"),
    ("ctrl", "Control"),
    ("alt", "Alt"),
    ("option", "Alt"),
    ("shift", "Shift"),
    ("super", "Super"),
    ("cmd", "Super"),
    ("command", "Super"),
    ("meta", "Super"),
];
const MODIFIER_ORDER: &[&str] = &["CommandOrControl", "Control", "Alt", "Shift", "Super"];
const NAMED_KEYS: &[(&str, &str)] = &[
    ("space", "Space"),
    ("enter", "Enter"),
    ("return", "Enter"),
    ("tab", "Tab"),
    ("backspace", "Backspace"),
    ("up", "Up"),
    ("down", "Down"),
    ("left", "Left"),
    ("right", "Right"),
    ("home", "Home"),
    ("end", "End"),
    ("pageup", "PageUp"),
    ("pagedown", "PageDown"),
];

fn invalid(reason: &str) -> AppError {
    AppError::InvalidInput(format!("Invalid shortcut: {reason}"))
}

fn normalise_key(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    if let Some((_, name)) = NAMED_KEYS.iter().find(|(k, _)| *k == lower) {
        return Some((*name).to_string());
    }
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase().to_string()),
        _ => {
            let number = lower.strip_prefix('f')?.parse::<u8>().ok()?;
            (1..=24).contains(&number).then(|| format!("F{number}"))
        }
    }
}

/// Canonical accelerator (`CommandOrControl+Shift+K`): at least one modifier,
/// exactly one key, and something the shortcut backend can parse.
pub fn validate_accelerator(input: &str) -> AppResult<String> {
    let mut modifiers: Vec<&'static str> = Vec::new();
    let mut key: Option<String> = None;
    for token in input.split('+').map(str::trim) {
        if token.is_empty() {
            return Err(invalid("empty part"));
        }
        let lower = token.to_lowercase();
        if let Some((_, canonical)) = MODIFIERS.iter().find(|(name, _)| *name == lower) {
            if !modifiers.contains(canonical) {
                modifiers.push(canonical);
            }
        } else if let Some(normalised) = normalise_key(token) {
            if key.replace(normalised).is_some() {
                return Err(invalid("more than one key"));
            }
        } else {
            return Err(invalid(&format!("unknown key `{token}`")));
        }
    }
    let key = key.ok_or_else(|| invalid("no key"))?;
    if modifiers.is_empty() {
        return Err(invalid("add a modifier such as Ctrl or ⌘"));
    }
    modifiers.sort_by_key(|m| MODIFIER_ORDER.iter().position(|o| o == m));
    let canonical = format!("{}+{key}", modifiers.join("+"));
    Shortcut::from_str(&canonical).map_err(|e| invalid(&e.to_string()))?;
    Ok(canonical)
}

/// The plugin with LocalDeck's handler: show the window, open the palette.
pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit(OPEN_PALETTE_EVENT, ());
            }
        })
        .build()
}

/// Switch from `old` to `new`. The new one is registered first; if that fails
/// the old one stays active and nothing changes.
pub fn apply<R: tauri::Runtime>(
    app: &AppHandle<R>,
    old: Option<&str>,
    new: Option<&str>,
) -> AppResult<()> {
    if old == new {
        return Ok(());
    }
    let manager = app.global_shortcut();
    if let Some(new) = new {
        let shortcut = Shortcut::from_str(new).map_err(|e| invalid(&e.to_string()))?;
        manager.register(shortcut).map_err(|e| {
            invalid(&format!(
                "{new} could not be registered (is another app using it?): {e}"
            ))
        })?;
    }
    if let Some(old) = old {
        if let Ok(shortcut) = Shortcut::from_str(old) {
            let _ = manager.unregister(shortcut);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_canonicalises_common_spellings() {
        for (input, want) in [
            ("CommandOrControl+Shift+K", "CommandOrControl+Shift+K"),
            ("shift+cmdorctrl+k", "CommandOrControl+Shift+K"),
            ("Ctrl+Alt+Space", "Control+Alt+Space"),
            ("cmd+option+1", "Alt+Super+1"),
            ("Ctrl + F12", "Control+F12"),
            ("Super+Left", "Super+Left"),
            ("Ctrl+Ctrl+P", "Control+P"),
        ] {
            assert_eq!(validate_accelerator(input).unwrap(), want, "{input}");
        }
    }

    #[test]
    fn rejects_bare_keys_bare_modifiers_and_garbage() {
        for bad in [
            "",
            "K",
            "F5",
            "Ctrl",
            "Ctrl+Shift",
            "Ctrl+K+L",
            "Ctrl+",
            "+K",
            "Ctrl+Banana",
            "Ctrl+F25",
            "Ctrl+F0",
            "Ctrl+é",
            "Ctrl+Shift+KK",
            "Ctrl++K",
        ] {
            let err = validate_accelerator(bad).unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::InvalidInput, "{bad:?}");
        }
    }

    #[test]
    fn canonical_results_are_idempotent() {
        let once = validate_accelerator("cmd+shift+p").unwrap();
        assert_eq!(validate_accelerator(&once).unwrap(), once);
    }
}
