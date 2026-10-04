//! Key/value settings stored as JSON values.

use super::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{Settings, Theme};
use rusqlite::params;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

const KEY_THEME: &str = "theme";
const KEY_LAUNCH_AT_LOGIN: &str = "launchAtLogin";
const KEY_PREFERRED_EDITOR: &str = "preferredEditor";
const KEY_FORGET_AFTER_DAYS: &str = "forgetAfterDays";

/// Decode a stored value; a missing or invalid one yields `default`.
fn decode<T: DeserializeOwned>(stored: &HashMap<String, String>, key: &str, default: T) -> T {
    let Some(raw) = stored.get(key) else {
        return default;
    };
    serde_json::from_str(raw).unwrap_or_else(|err| {
        tracing::warn!(key, %err, "invalid stored setting; using default");
        default
    })
}

fn encode<T: serde::Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value)
        .map_err(|err| AppError::Database(format!("could not encode setting: {err}")))
}

impl Database {
    /// Defaults overlaid with stored keys. An invalid stored value falls back
    /// to that key's default; this never errors on bad data.
    pub fn load_settings(&self) -> AppResult<Settings> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let stored: HashMap<String, String> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let defaults = Settings::default();
        Ok(Settings {
            theme: decode::<Theme>(&stored, KEY_THEME, defaults.theme),
            launch_at_login: decode(&stored, KEY_LAUNCH_AT_LOGIN, defaults.launch_at_login),
            preferred_editor: decode(&stored, KEY_PREFERRED_EDITOR, defaults.preferred_editor),
            forget_after_days: decode(&stored, KEY_FORGET_AFTER_DAYS, defaults.forget_after_days),
        })
    }

    /// Persist every key in one transaction.
    pub fn save_settings(&mut self, settings: &Settings) -> AppResult<()> {
        let entries = [
            (KEY_THEME, encode(&settings.theme)?),
            (KEY_LAUNCH_AT_LOGIN, encode(&settings.launch_at_login)?),
            (KEY_PREFERRED_EDITOR, encode(&settings.preferred_editor)?),
            (KEY_FORGET_AFTER_DAYS, encode(&settings.forget_after_days)?),
        ];
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )?;
            for (key, value) in &entries {
                stmt.execute(params![key, value])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_database_yields_defaults() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn settings_round_trip_and_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localdeck.db");
        let custom = Settings {
            theme: Theme::Dark,
            launch_at_login: true,
            preferred_editor: Some("vscode".into()),
            forget_after_days: Some(30),
        };
        {
            let mut db = Database::open(&path).unwrap();
            db.save_settings(&custom).unwrap();
            assert_eq!(db.load_settings().unwrap(), custom);
        }
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.load_settings().unwrap(), custom);

        // Saving `None` values must clear previously stored ones.
        let reset = Settings {
            theme: Theme::Light,
            ..Settings::default()
        };
        db.save_settings(&reset).unwrap();
        assert_eq!(db.load_settings().unwrap(), reset);
        let rows: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 4);
    }

    #[test]
    fn invalid_values_fall_back_per_key() {
        let mut db = Database::open_in_memory().unwrap();
        db.save_settings(&Settings {
            theme: Theme::Dark,
            launch_at_login: true,
            preferred_editor: Some("zed".into()),
            forget_after_days: Some(14),
        })
        .unwrap();
        for (key, raw) in [
            ("theme", "\"neon\""),
            ("launchAtLogin", "\"yes\""),
            ("forgetAfterDays", "-3"),
        ] {
            db.conn
                .execute("UPDATE settings SET value = ?2 WHERE key = ?1", [key, raw])
                .unwrap();
        }
        db.conn
            .execute(
                "UPDATE settings SET value = 'not json' WHERE key = 'preferredEditor'",
                [],
            )
            .unwrap();

        // Every key is invalid, so every key falls back to its own default.
        assert_eq!(db.load_settings().unwrap(), Settings::default());

        // A single bad key must not discard the valid ones.
        db.save_settings(&Settings {
            theme: Theme::Dark,
            forget_after_days: Some(7),
            ..Settings::default()
        })
        .unwrap();
        db.conn
            .execute(
                "UPDATE settings SET value = '{' WHERE key = 'launchAtLogin'",
                [],
            )
            .unwrap();
        let loaded = db.load_settings().unwrap();
        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(loaded.forget_after_days, Some(7));
        assert!(!loaded.launch_at_login);
    }
}
