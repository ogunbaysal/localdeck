//! Project identity and user overrides.

use super::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{ComponentKind, LaunchProfile, PrimaryCommand, ProjectId};
use rusqlite::{params, OptionalExtension, Row, Transaction};
use std::collections::BTreeMap;

/// A persisted project: identity, flags and user overrides (no runtime data).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRecord {
    pub id: ProjectId,
    pub root_path: String,
    /// User-chosen display name; `None` means derive one from the folder.
    pub display_name: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub favorite: bool,
    pub ignored: bool,
    pub primary_url: Option<String>,
    pub primary_command: Option<PrimaryCommand>,
    pub preferred_editor: Option<String>,
    pub preferred_terminal: Option<String>,
    pub auto_restart: bool,
    pub launch_profiles: Vec<LaunchProfile>,
    pub component_kinds: BTreeMap<String, ComponentKind>,
}

const SELECT_PROJECT: &str = "SELECT p.id, p.root_path, p.display_name_override, p.first_seen_at,
        p.last_seen_at, p.favorite, p.ignored,
        o.primary_url, o.primary_command, o.preferred_editor, o.component_overrides,
        o.preferred_terminal, o.auto_restart, o.launch_profiles
    FROM projects p LEFT JOIN project_overrides o ON o.project_id = p.id";

/// Parse a JSON column. A corrupt value degrades to `None` instead of making
/// the whole project list unreadable.
fn parse_json<T: serde::de::DeserializeOwned>(
    id: ProjectId,
    column: &str,
    raw: Option<String>,
) -> Option<T> {
    let raw = raw?;
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::warn!(project = id, column, %err, "ignoring unreadable stored value");
            None
        }
    }
}

fn record_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let id: ProjectId = row.get(0)?;
    Ok(ProjectRecord {
        id,
        root_path: row.get(1)?,
        display_name: row.get(2)?,
        first_seen_at: row.get(3)?,
        last_seen_at: row.get(4)?,
        favorite: row.get(5)?,
        ignored: row.get(6)?,
        primary_url: row.get(7)?,
        primary_command: parse_json(id, "primary_command", row.get(8)?),
        preferred_editor: row.get(9)?,
        component_kinds: parse_json(id, "component_overrides", row.get(10)?).unwrap_or_default(),
        preferred_terminal: row.get(11)?,
        auto_restart: row.get::<_, Option<bool>>(12)?.unwrap_or(false),
        launch_profiles: parse_json(id, "launch_profiles", row.get(13)?).unwrap_or_default(),
    })
}

/// Trim text; blank becomes `None`.
fn normalise(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn require_project(tx: &Transaction<'_>, id: ProjectId) -> AppResult<()> {
    let found = tx
        .query_row("SELECT 1 FROM projects WHERE id = ?1", [id], |_| Ok(()))
        .optional()?;
    found.ok_or(AppError::ProjectNotFound { id })
}

impl Database {
    /// Insert a project (first = last seen = `now_ms`) or return the existing
    /// one unchanged.
    pub fn upsert_project(&mut self, root_path: &str, now_ms: i64) -> AppResult<ProjectRecord> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO projects (root_path, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?2) ON CONFLICT(root_path) DO NOTHING",
            params![root_path, now_ms],
        )?;
        let record = tx.query_row(
            &format!("{SELECT_PROJECT} WHERE p.root_path = ?1"),
            [root_path],
            record_from_row,
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO project_overrides (project_id) VALUES (?1)",
            [record.id],
        )?;
        tx.commit()?;
        Ok(record)
    }

    pub fn touch_project(&mut self, id: ProjectId, now_ms: i64) -> AppResult<()> {
        self.update_project_column("last_seen_at", id, now_ms)
    }

    pub fn list_projects(&self) -> AppResult<Vec<ProjectRecord>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SELECT_PROJECT} ORDER BY p.id"))?;
        let rows = stmt.query_map([], record_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_project(&self, id: ProjectId) -> AppResult<ProjectRecord> {
        self.conn
            .query_row(
                &format!("{SELECT_PROJECT} WHERE p.id = ?1"),
                [id],
                record_from_row,
            )
            .optional()?
            .ok_or(AppError::ProjectNotFound { id })
    }

    pub fn set_display_name(&mut self, id: ProjectId, name: Option<&str>) -> AppResult<()> {
        self.update_project_column("display_name_override", id, normalise(name))
    }

    pub fn set_ignored(&mut self, id: ProjectId, ignored: bool) -> AppResult<()> {
        self.update_project_column("ignored", id, ignored)
    }

    pub fn set_favorite(&mut self, id: ProjectId, favorite: bool) -> AppResult<()> {
        self.update_project_column("favorite", id, favorite)
    }

    pub fn set_primary_url(&mut self, id: ProjectId, url: Option<&str>) -> AppResult<()> {
        self.update_override_column("primary_url", id, normalise(url))
    }

    pub fn set_primary_command(
        &mut self,
        id: ProjectId,
        command: Option<&PrimaryCommand>,
    ) -> AppResult<()> {
        let json = command
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| AppError::Database(format!("could not encode command: {err}")))?;
        self.update_override_column("primary_command", id, json)
    }

    pub fn set_preferred_editor(&mut self, id: ProjectId, editor: Option<&str>) -> AppResult<()> {
        self.update_override_column("preferred_editor", id, normalise(editor))
    }

    pub fn set_preferred_terminal(
        &mut self,
        id: ProjectId,
        terminal: Option<&str>,
    ) -> AppResult<()> {
        self.update_override_column("preferred_terminal", id, normalise(terminal))
    }

    pub fn set_auto_restart(&mut self, id: ProjectId, enabled: bool) -> AppResult<()> {
        self.update_override_column("auto_restart", id, enabled)
    }

    /// Replace all launch profiles (empty clears them).
    pub fn set_launch_profiles(
        &mut self,
        id: ProjectId,
        profiles: &[LaunchProfile],
    ) -> AppResult<()> {
        let json = if profiles.is_empty() {
            None
        } else {
            Some(serde_json::to_string(profiles).map_err(|err| {
                AppError::Database(format!("could not encode launch profiles: {err}"))
            })?)
        };
        self.update_override_column("launch_profiles", id, json)
    }

    /// Set, replace (`Some`) or remove (`None`) one component-kind override.
    pub fn set_component_kind(
        &mut self,
        id: ProjectId,
        key: &str,
        kind: Option<ComponentKind>,
    ) -> AppResult<()> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::InvalidInput(
                "component key must not be empty".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        require_project(&tx, id)?;
        tx.execute(
            "INSERT OR IGNORE INTO project_overrides (project_id) VALUES (?1)",
            [id],
        )?;
        let raw: Option<String> = tx.query_row(
            "SELECT component_overrides FROM project_overrides WHERE project_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let mut map: BTreeMap<String, ComponentKind> =
            parse_json(id, "component_overrides", raw).unwrap_or_default();
        match kind {
            Some(kind) => {
                map.insert(key.to_owned(), kind);
            }
            None => {
                map.remove(key);
            }
        }
        let json = if map.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&map)
                    .map_err(|err| AppError::Database(format!("could not encode kinds: {err}")))?,
            )
        };
        tx.execute(
            "UPDATE project_overrides SET component_overrides = ?2 WHERE project_id = ?1",
            params![id, json],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Delete a project; overrides and activity cascade.
    pub fn delete_project(&mut self, id: ProjectId) -> AppResult<()> {
        let removed = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
        if removed == 0 {
            return Err(AppError::ProjectNotFound { id });
        }
        Ok(())
    }

    /// Delete several projects in one transaction; returns how many existed.
    pub fn delete_projects(&mut self, ids: &[ProjectId]) -> AppResult<usize> {
        let tx = self.conn.transaction()?;
        let mut removed = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM projects WHERE id = ?1")?;
            for id in ids {
                removed += stmt.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Forget projects last seen before the cutoff, except ignored or
    /// favourited ones. Returns how many were deleted.
    pub fn prune_stale_projects(&mut self, last_seen_before_ms: i64) -> AppResult<usize> {
        Ok(self.conn.execute(
            "DELETE FROM projects
             WHERE last_seen_at < ?1 AND ignored = 0 AND favorite = 0",
            [last_seen_before_ms],
        )?)
    }

    /// `column` is always a compile-time constant from this module.
    fn update_project_column(
        &mut self,
        column: &'static str,
        id: ProjectId,
        value: impl rusqlite::ToSql,
    ) -> AppResult<()> {
        let changed = self.conn.execute(
            &format!("UPDATE projects SET {column} = ?2 WHERE id = ?1"),
            params![id, value],
        )?;
        if changed == 0 {
            return Err(AppError::ProjectNotFound { id });
        }
        Ok(())
    }

    /// `column` is always a compile-time constant from this module.
    fn update_override_column(
        &mut self,
        column: &'static str,
        id: ProjectId,
        value: impl rusqlite::ToSql,
    ) -> AppResult<()> {
        let tx = self.conn.transaction()?;
        require_project(&tx, id)?;
        tx.execute(
            "INSERT OR IGNORE INTO project_overrides (project_id) VALUES (?1)",
            [id],
        )?;
        tx.execute(
            &format!("UPDATE project_overrides SET {column} = ?2 WHERE project_id = ?1"),
            params![id, value],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityKind, ActivityMetadata};
    use crate::persistence::NewActivity;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn command() -> PrimaryCommand {
        PrimaryCommand {
            program: "bun".into(),
            args: vec!["run".into(), "dev server".into(), "--name=my app".into()],
            cwd: "/tmp/Projeler/My App".into(),
            label: "bun run dev server".into(),
            script: Some("dev server".into()),
        }
    }

    #[test]
    fn upsert_is_idempotent_and_unique_by_root_path() {
        let mut db = db();
        let paths = [
            "/tmp/plain",
            "/tmp/with space/app",
            "/tmp/Projeler/Anılarım 日本語",
            "/tmp/it's a project",
        ];
        for path in paths {
            let first = db.upsert_project(path, 100).unwrap();
            let second = db.upsert_project(path, 999).unwrap();
            assert_eq!(first, second, "second upsert must not change anything");
            assert_eq!(first.root_path, path);
            assert_eq!((first.first_seen_at, first.last_seen_at), (100, 100));
        }
        let all = db.list_projects().unwrap();
        assert_eq!(all.len(), paths.len());
        assert!(all.windows(2).all(|w| w[0].id < w[1].id));
    }

    #[test]
    fn upsert_creates_override_row_lazily_once() {
        let mut db = db();
        let project = db.upsert_project("/tmp/a", 1).unwrap();
        db.upsert_project("/tmp/a", 2).unwrap();
        let rows: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM project_overrides WHERE project_id = ?1",
                [project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn touch_updates_last_seen_only() {
        let mut db = db();
        let project = db.upsert_project("/tmp/a", 100).unwrap();
        db.touch_project(project.id, 500).unwrap();
        let loaded = db.get_project(project.id).unwrap();
        assert_eq!(loaded.first_seen_at, 100);
        assert_eq!(loaded.last_seen_at, 500);
    }

    #[test]
    fn display_name_and_scalar_overrides_round_trip_and_clear() {
        let mut db = db();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;

        db.set_display_name(id, Some("  Café Pro  ")).unwrap();
        db.set_primary_url(id, Some(" http://localhost:5173/app "))
            .unwrap();
        db.set_preferred_editor(id, Some("zed")).unwrap();
        db.set_primary_command(id, Some(&command())).unwrap();

        let loaded = db.get_project(id).unwrap();
        assert_eq!(loaded.display_name.as_deref(), Some("Café Pro"));
        assert_eq!(
            loaded.primary_url.as_deref(),
            Some("http://localhost:5173/app")
        );
        assert_eq!(loaded.preferred_editor.as_deref(), Some("zed"));
        assert_eq!(loaded.primary_command, Some(command()));

        db.set_display_name(id, Some("   ")).unwrap();
        db.set_primary_url(id, None).unwrap();
        db.set_preferred_editor(id, Some("")).unwrap();
        db.set_primary_command(id, None).unwrap();
        let cleared = db.get_project(id).unwrap();
        assert_eq!(cleared.display_name, None);
        assert_eq!(cleared.primary_url, None);
        assert_eq!(cleared.preferred_editor, None);
        assert_eq!(cleared.primary_command, None);
    }

    #[test]
    fn component_kind_set_replace_and_clear() {
        let mut db = db();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;

        db.set_component_kind(id, "web", Some(ComponentKind::Web))
            .unwrap();
        db.set_component_kind(id, "docs site", Some(ComponentKind::Api))
            .unwrap();
        let kinds = db.get_project(id).unwrap().component_kinds;
        assert_eq!(kinds.len(), 2);
        assert_eq!(kinds["web"], ComponentKind::Web);

        db.set_component_kind(id, "web", Some(ComponentKind::Worker))
            .unwrap();
        assert_eq!(
            db.get_project(id).unwrap().component_kinds["web"],
            ComponentKind::Worker
        );

        db.set_component_kind(id, "web", None).unwrap();
        db.set_component_kind(id, "docs site", None).unwrap();
        assert!(db.get_project(id).unwrap().component_kinds.is_empty());
        // Removing something that is not there is a no-op.
        db.set_component_kind(id, "web", None).unwrap();

        assert!(matches!(
            db.set_component_kind(id, "  ", Some(ComponentKind::Web)),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn overrides_survive_a_repeated_upsert() {
        let mut db = db();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;
        db.set_primary_command(id, Some(&command())).unwrap();
        db.set_favorite(id, true).unwrap();
        let again = db.upsert_project("/tmp/a", 50).unwrap();
        assert_eq!(again.primary_command, Some(command()));
        assert!(again.favorite);
    }

    #[test]
    fn ignored_and_favorite_flags_toggle() {
        let mut db = db();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;
        let fresh = db.get_project(id).unwrap();
        assert!(!fresh.ignored && !fresh.favorite);

        db.set_ignored(id, true).unwrap();
        db.set_favorite(id, true).unwrap();
        let on = db.get_project(id).unwrap();
        assert!(on.ignored && on.favorite);

        db.set_ignored(id, false).unwrap();
        let off = db.get_project(id).unwrap();
        assert!(!off.ignored && off.favorite);
    }

    #[test]
    fn missing_project_errors_everywhere() {
        let mut db = db();
        let missing = 4242;
        let is_missing = |result: AppResult<()>| matches!(result, Err(AppError::ProjectNotFound { id }) if id == missing);
        assert!(matches!(
            db.get_project(missing),
            Err(AppError::ProjectNotFound { id }) if id == missing
        ));
        assert!(is_missing(db.touch_project(missing, 1)));
        assert!(is_missing(db.set_display_name(missing, Some("x"))));
        assert!(is_missing(db.set_ignored(missing, true)));
        assert!(is_missing(db.set_favorite(missing, true)));
        assert!(is_missing(db.set_primary_url(missing, Some("http://x"))));
        assert!(is_missing(
            db.set_primary_command(missing, Some(&command()))
        ));
        assert!(is_missing(db.set_preferred_editor(missing, Some("zed"))));
        assert!(is_missing(db.set_component_kind(
            missing,
            "web",
            Some(ComponentKind::Web)
        )));
        assert!(is_missing(db.delete_project(missing)));
        // Nothing was created as a side effect.
        assert!(db.list_projects().unwrap().is_empty());
        let rows: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM project_overrides", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn delete_cascades_overrides_and_activity() {
        let mut db = db();
        let keep = db.upsert_project("/tmp/keep", 1).unwrap().id;
        let gone = db.upsert_project("/tmp/gone", 1).unwrap().id;
        db.set_primary_url(gone, Some("http://localhost:1"))
            .unwrap();
        let event = |project_id| NewActivity {
            project_id: Some(project_id),
            kind: ActivityKind::ProjectStarted,
            metadata: ActivityMetadata::default(),
            created_at: 1,
        };
        db.insert_activity(&[event(keep), event(gone), event(gone)])
            .unwrap();

        db.delete_project(gone).unwrap();

        assert!(matches!(
            db.get_project(gone),
            Err(AppError::ProjectNotFound { .. })
        ));
        let count = |sql: &str| -> i64 { db.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM project_overrides"), 1);
        assert_eq!(count("SELECT COUNT(*) FROM activity"), 1);
        assert_eq!(
            count("SELECT COUNT(*) FROM activity WHERE project_id IS NOT NULL"),
            1
        );
        assert_eq!(db.get_project(keep).unwrap().root_path, "/tmp/keep");
    }

    #[test]
    fn delete_projects_counts_only_existing_rows() {
        let mut db = db();
        let a = db.upsert_project("/tmp/a", 1).unwrap().id;
        let b = db.upsert_project("/tmp/b", 1).unwrap().id;
        let c = db.upsert_project("/tmp/c", 1).unwrap().id;
        assert_eq!(db.delete_projects(&[a, b, 9999]).unwrap(), 2);
        assert_eq!(db.delete_projects(&[]).unwrap(), 0);
        let left = db.list_projects().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, c);
    }

    #[test]
    fn prune_stale_respects_ignored_and_favorite() {
        let mut db = db();
        let stale = db.upsert_project("/tmp/stale", 100).unwrap().id;
        let ignored = db.upsert_project("/tmp/ignored", 100).unwrap().id;
        let favorite = db.upsert_project("/tmp/favorite", 100).unwrap().id;
        let fresh = db.upsert_project("/tmp/fresh", 100).unwrap().id;
        let boundary = db.upsert_project("/tmp/boundary", 100).unwrap().id;
        db.set_ignored(ignored, true).unwrap();
        db.set_favorite(favorite, true).unwrap();
        db.touch_project(fresh, 1_000).unwrap();
        db.touch_project(boundary, 500).unwrap();

        // Strictly older than the cutoff: `boundary` (== 500) survives.
        let removed = db.prune_stale_projects(500).unwrap();
        assert_eq!(removed, 1);
        let ids: Vec<_> = db.list_projects().unwrap().iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![ignored, favorite, fresh, boundary]);
        assert!(!ids.contains(&stale));
    }

    #[test]
    fn corrupt_json_column_degrades_instead_of_failing() {
        let mut db = db();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;
        db.conn
            .execute(
                "UPDATE project_overrides SET primary_command = '{nope', component_overrides = '[1]'",
                [],
            )
            .unwrap();
        let loaded = db.get_project(id).unwrap();
        assert_eq!(loaded.primary_command, None);
        assert!(loaded.component_kinds.is_empty());
        assert_eq!(db.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn terminal_auto_restart_and_launch_profiles_round_trip() {
        use crate::models::{LaunchProfile, PrimaryCommand};
        let mut db = Database::open_in_memory().unwrap();
        let id = db.upsert_project("/tmp/ünï code/app", 1).unwrap().id;
        let record = db.get_project(id).unwrap();
        assert!(!record.auto_restart && record.launch_profiles.is_empty());
        let profile = LaunchProfile {
            id: "p1".into(),
            name: "Web + API".into(),
            commands: vec![PrimaryCommand {
                program: "bun".into(),
                args: vec!["run".into(), "dev server".into()],
                cwd: "/tmp/ünï code/app".into(),
                label: "bun run dev server".into(),
                script: Some("pkg:dev server".into()),
            }],
        };
        db.set_preferred_terminal(id, Some(" ghostty ")).unwrap();
        db.set_auto_restart(id, true).unwrap();
        db.set_launch_profiles(id, std::slice::from_ref(&profile))
            .unwrap();
        let record = db.get_project(id).unwrap();
        assert_eq!(record.preferred_terminal.as_deref(), Some("ghostty"));
        assert!(record.auto_restart);
        assert_eq!(record.launch_profiles, vec![profile]);
        db.set_launch_profiles(id, &[]).unwrap();
        db.set_preferred_terminal(id, None).unwrap();
        db.set_auto_restart(id, false).unwrap();
        let record = db.get_project(id).unwrap();
        assert!(
            record.launch_profiles.is_empty()
                && record.preferred_terminal.is_none()
                && !record.auto_restart
        );
        assert_eq!(
            db.set_auto_restart(999, true).unwrap_err().code(),
            crate::error::ErrorCode::ProjectNotFound
        );
    }
}
