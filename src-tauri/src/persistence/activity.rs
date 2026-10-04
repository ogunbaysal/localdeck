//! Bounded, global activity log.

use super::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{ActivityEvent, ActivityKind, ActivityMetadata, ProjectId};
use rusqlite::{params, Row};

/// Default maximum number of activity rows kept across all projects.
pub const ACTIVITY_LIMIT: usize = 500;

/// An event to record. `message` is derived on read via [`ActivityEvent::describe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewActivity {
    pub project_id: Option<ProjectId>,
    pub kind: ActivityKind,
    pub metadata: ActivityMetadata,
    pub created_at: i64,
}

type RawActivity = (i64, Option<ProjectId>, String, String, i64);

fn raw_from_row(row: &Row<'_>) -> rusqlite::Result<RawActivity> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn event_from_raw(
    (id, project_id, kind, metadata, created_at): RawActivity,
) -> Option<ActivityEvent> {
    let Some(kind) = ActivityKind::from_db(&kind) else {
        tracing::warn!(id, kind, "skipping activity row of unknown kind");
        return None;
    };
    let metadata: ActivityMetadata = serde_json::from_str(&metadata).unwrap_or_else(|err| {
        tracing::warn!(id, %err, "activity metadata unreadable; using defaults");
        ActivityMetadata::default()
    });
    Some(ActivityEvent {
        id,
        project_id,
        kind,
        message: ActivityEvent::describe(kind, &metadata),
        metadata,
        created_at,
    })
}

impl Database {
    /// Insert all events and trim the oldest rows beyond the retention limit,
    /// in one transaction. Returns every inserted event (even one that the
    /// trim immediately dropped because the batch alone exceeds the limit).
    pub fn insert_activity(&mut self, events: &[NewActivity]) -> AppResult<Vec<ActivityEvent>> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(self.activity_limit).unwrap_or(i64::MAX);
        let tx = self.conn.transaction()?;
        let mut inserted = Vec::with_capacity(events.len());
        {
            let mut stmt = tx.prepare(
                "INSERT INTO activity (project_id, event_type, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for event in events {
                let metadata = serde_json::to_string(&event.metadata).map_err(|err| {
                    AppError::Database(format!("could not encode activity metadata: {err}"))
                })?;
                stmt.execute(params![
                    event.project_id,
                    event.kind.as_db(),
                    metadata,
                    event.created_at
                ])?;
                inserted.push(ActivityEvent {
                    id: tx.last_insert_rowid(),
                    project_id: event.project_id,
                    kind: event.kind,
                    message: ActivityEvent::describe(event.kind, &event.metadata),
                    metadata: event.metadata.clone(),
                    created_at: event.created_at,
                });
            }
        }
        tx.execute(
            "DELETE FROM activity
             WHERE id NOT IN (SELECT id FROM activity ORDER BY id DESC LIMIT ?1)",
            [limit],
        )?;
        tx.commit()?;
        Ok(inserted)
    }

    /// Most recent events, newest first.
    pub fn recent_activity(&self, limit: usize) -> AppResult<Vec<ActivityEvent>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, event_type, metadata, created_at
             FROM activity ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], raw_from_row)?;
        let mut events = Vec::new();
        for raw in rows {
            events.extend(event_from_raw(raw?));
        }
        Ok(events)
    }

    pub fn activity_count(&self) -> AppResult<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(project_id: Option<ProjectId>, port: u16, created_at: i64) -> NewActivity {
        NewActivity {
            project_id,
            kind: ActivityKind::PortOpened,
            metadata: ActivityMetadata {
                port: Some(port),
                ..ActivityMetadata::default()
            },
            created_at,
        }
    }

    #[test]
    fn default_limit_is_500() {
        assert_eq!(ACTIVITY_LIMIT, 500);
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.activity_limit, 500);
    }

    #[test]
    fn retention_is_bounded_and_drops_oldest() {
        let mut db = Database::open_in_memory().unwrap().with_activity_limit(5);
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;

        let first = db
            .insert_activity(&[
                event(Some(id), 1, 10),
                event(Some(id), 2, 11),
                event(None, 3, 12),
            ])
            .unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(db.activity_count().unwrap(), 3);

        // 3 + 4 = 7 rows inserted; the two oldest must be trimmed in the same call.
        let second = db
            .insert_activity(
                &(4..=7)
                    .map(|p| event(Some(id), p, 20 + i64::from(p)))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(second.len(), 4);
        assert_eq!(db.activity_count().unwrap(), 5);

        let ports: Vec<_> = db
            .recent_activity(100)
            .unwrap()
            .iter()
            .map(|e| e.metadata.port.unwrap())
            .collect();
        assert_eq!(ports, vec![7, 6, 5, 4, 3]);
    }

    #[test]
    fn batch_larger_than_limit_keeps_only_newest() {
        let mut db = Database::open_in_memory().unwrap().with_activity_limit(2);
        let batch: Vec<_> = (1..=4).map(|p| event(None, p, i64::from(p))).collect();
        assert_eq!(db.insert_activity(&batch).unwrap().len(), 4);
        let ports: Vec<_> = db
            .recent_activity(10)
            .unwrap()
            .iter()
            .map(|e| e.metadata.port.unwrap())
            .collect();
        assert_eq!(ports, vec![4, 3]);
    }

    #[test]
    fn failed_batch_rolls_back_entirely() {
        let mut db = Database::open_in_memory().unwrap().with_activity_limit(5);
        db.insert_activity(&[event(None, 1, 1)]).unwrap();
        let result = db.insert_activity(&[event(None, 2, 2), event(Some(9999), 3, 3)]);
        assert!(matches!(result, Err(AppError::Database(_))));
        assert_eq!(db.activity_count().unwrap(), 1);
    }

    #[test]
    fn recent_activity_is_newest_first_and_limited() {
        let mut db = Database::open_in_memory().unwrap();
        let batch: Vec<_> = (1..=6).map(|p| event(None, p, i64::from(p))).collect();
        db.insert_activity(&batch).unwrap();
        let recent = db.recent_activity(3).unwrap();
        let ports: Vec<_> = recent.iter().map(|e| e.metadata.port.unwrap()).collect();
        assert_eq!(ports, vec![6, 5, 4]);
        assert!(recent.windows(2).all(|w| w[0].id > w[1].id));
        assert!(db.recent_activity(0).unwrap().is_empty());
        assert!(db.insert_activity(&[]).unwrap().is_empty());
    }

    #[test]
    fn metadata_and_message_round_trip() {
        let mut db = Database::open_in_memory().unwrap();
        let id = db.upsert_project("/tmp/a", 1).unwrap().id;
        let exited = NewActivity {
            project_id: Some(id),
            kind: ActivityKind::ManagedExited,
            metadata: ActivityMetadata {
                port: Some(3000),
                component: Some("web app".into()),
                exit_code: Some(137),
                detected: true,
            },
            created_at: 42,
        };
        let detected = NewActivity {
            project_id: Some(id),
            kind: ActivityKind::ProjectStarted,
            metadata: ActivityMetadata {
                detected: true,
                ..ActivityMetadata::default()
            },
            created_at: 43,
        };
        let returned = db
            .insert_activity(&[exited.clone(), detected.clone()])
            .unwrap();
        let stored = db.recent_activity(10).unwrap();

        // `recent_activity` is newest-first; `insert_activity` returns input order.
        assert_eq!(stored.iter().rev().cloned().collect::<Vec<_>>(), returned);
        assert_eq!(returned[0].kind, ActivityKind::ManagedExited);
        assert_eq!(returned[0].metadata, exited.metadata);
        assert_eq!(returned[0].message, "Managed command exited with code 137");
        assert_eq!(returned[0].project_id, Some(id));
        assert_eq!(returned[0].created_at, 42);
        assert_eq!(returned[1].message, "Detected running");
    }

    #[test]
    fn unknown_kind_rows_are_skipped() {
        let mut db = Database::open_in_memory().unwrap();
        db.insert_activity(&[event(None, 1, 1)]).unwrap();
        db.conn
            .execute(
                "INSERT INTO activity (event_type, created_at) VALUES ('from_the_future', 2)",
                [],
            )
            .unwrap();
        let recent = db.recent_activity(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].metadata.port, Some(1));
    }
}
