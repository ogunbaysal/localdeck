//! Connection management and schema migrations.

use super::activity::ACTIVITY_LIMIT;
use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

/// Ordered schema migrations. Index `i` upgrades `user_version` `i` → `i + 1`.
/// Append only; never edit a shipped entry.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    "CREATE TABLE projects (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        root_path TEXT NOT NULL UNIQUE,
        display_name_override TEXT,
        first_seen_at INTEGER NOT NULL,
        last_seen_at INTEGER NOT NULL,
        favorite INTEGER NOT NULL DEFAULT 0,
        ignored INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE project_overrides (
        project_id INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
        primary_url TEXT,
        primary_command TEXT,
        preferred_editor TEXT,
        component_overrides TEXT
    );
    CREATE TABLE activity (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
        event_type TEXT NOT NULL,
        metadata TEXT NOT NULL DEFAULT '{}',
        created_at INTEGER NOT NULL
    );
    CREATE INDEX idx_activity_project ON activity(project_id, id DESC);
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
    // 2: launch profiles, auto-restart, terminal choice
    "ALTER TABLE project_overrides ADD COLUMN launch_profiles TEXT;
    ALTER TABLE project_overrides ADD COLUMN auto_restart INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE project_overrides ADD COLUMN preferred_terminal TEXT;",
];

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle to LocalDeck's SQLite database. All access is synchronous; writers
/// take `&mut self`, so callers serialise through one owner (a mutex in app state).
pub struct Database {
    pub(super) conn: Connection,
    pub(super) activity_limit: usize,
}

impl Database {
    /// Open (creating parent directories and the file) and migrate.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|err| {
                AppError::Database(format!(
                    "could not create database directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let mut db = Self {
            conn,
            activity_limit: ACTIVITY_LIMIT,
        };
        db.migrate()?;
        Ok(db)
    }

    /// Override the activity retention bound (tests).
    pub fn with_activity_limit(mut self, limit: usize) -> Self {
        self.activity_limit = limit;
        self
    }

    /// Run raw SQL (tests that need to provoke failures).
    #[cfg(test)]
    pub(crate) fn execute_for_tests(&self, sql: &str) -> AppResult<()> {
        self.conn.execute_batch(sql)?;
        Ok(())
    }

    /// Current `PRAGMA user_version`.
    pub fn schema_version(&self) -> AppResult<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    fn migrate(&mut self) -> AppResult<()> {
        let current = self.schema_version()?;
        let known = MIGRATIONS.len() as i64;
        if current > known {
            return Err(AppError::Database(format!(
                "database schema version {current} is newer than this LocalDeck supports ({known}); \
                 update LocalDeck to open it"
            )));
        }
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
            tracing::info!(version = index + 1, "applied database migration");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(db: &Database, name: &str) -> bool {
        db.conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            == 1
    }

    #[test]
    fn empty_database_migrates_to_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("nested/deeper/localdeck.db")).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        for table in ["projects", "project_overrides", "activity", "settings"] {
            assert!(table_exists(&db, table), "missing table {table}");
        }
    }

    #[test]
    fn file_database_uses_wal_and_foreign_keys() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("localdeck.db")).unwrap();
        let mode: String = db
            .conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let fk: i64 = db
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn in_memory_database_enforces_foreign_keys() {
        let db = Database::open_in_memory().unwrap();
        let fk: i64 = db
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let orphan = db.conn.execute(
            "INSERT INTO activity (project_id, event_type, created_at) VALUES (999, 'x', 1)",
            [],
        );
        assert!(orphan.is_err(), "orphan activity row must be rejected");
    }

    #[test]
    fn reopening_is_idempotent_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localdeck.db");
        let id = {
            let mut db = Database::open(&path).unwrap();
            db.upsert_project("/tmp/keep-me", 10).unwrap().id
        };
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let projects = db.list_projects().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, id);
        assert_eq!(projects[0].root_path, "/tmp/keep-me");
    }

    #[test]
    fn newer_schema_version_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localdeck.db");
        drop(Database::open(&path).unwrap());
        let newer = MIGRATIONS.len() as i64 + 1;
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", newer)
            .unwrap();
        match Database::open(&path) {
            Err(AppError::Database(message)) => assert!(message.contains("newer"), "{message}"),
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("newer schema must be rejected"),
        }
    }

    #[test]
    fn a_v1_database_upgrades_in_place_keeping_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localdeck.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO projects (root_path, first_seen_at, last_seen_at, favorite) VALUES ('/work/old app', 1, 2, 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO project_overrides (project_id, primary_url) VALUES (1, 'http://localhost:3000')",
                [],
            )
            .unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let record = db.get_project(1).unwrap();
        assert!(record.favorite);
        assert_eq!(record.primary_url.as_deref(), Some("http://localhost:3000"));
        assert!(!record.auto_restart && record.launch_profiles.is_empty());
        assert!(record.preferred_terminal.is_none());
    }
}
