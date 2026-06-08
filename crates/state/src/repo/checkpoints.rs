//! Checkpoint repository: per-thread crash recovery snapshots.

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Database, DbError};

/// Checkpoint row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRow {
    /// Owning thread.
    pub thread_id: String,
    /// Checkpoint id (default `"latest"`).
    pub checkpoint_id: String,
    /// Serialized snapshot.
    pub state_json: String,
    /// Insert time (unix ms).
    pub created_at: i64,
}

/// Insert / replace payload.
#[derive(Debug, Clone)]
pub struct CheckpointInsert {
    /// Owning thread.
    pub thread_id: String,
    /// Checkpoint id.
    pub checkpoint_id: String,
    /// Snapshot JSON.
    pub state_json: String,
}

/// Checkpoint repository.
pub struct CheckpointRepo<'a> {
    db: &'a Database,
}

impl<'a> CheckpointRepo<'a> {
    /// Construct.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert or replace a checkpoint.
    pub fn put(&self, input: CheckpointInsert) -> Result<(), DbError> {
        let now = Utc::now().timestamp_millis();
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO checkpoints (thread_id, checkpoint_id, state_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(thread_id, checkpoint_id) DO UPDATE SET
               state_json = excluded.state_json,
               created_at = excluded.created_at",
            params![input.thread_id, input.checkpoint_id, input.state_json, now],
        )?;
        Ok(())
    }

    /// Read a checkpoint, returns `None` if not present.
    pub fn get(
        &self,
        thread_id: &str,
        checkpoint_id: &str,
    ) -> Result<Option<CheckpointRow>, DbError> {
        let conn = self.db.conn();
        conn.query_row(
            "SELECT thread_id, checkpoint_id, state_json, created_at
             FROM checkpoints WHERE thread_id = ?1 AND checkpoint_id = ?2",
            params![thread_id, checkpoint_id],
            |row| {
                Ok(CheckpointRow {
                    thread_id: row.get(0)?,
                    checkpoint_id: row.get(1)?,
                    state_json: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Delete a checkpoint (call when turn completes successfully).
    pub fn delete(&self, thread_id: &str, checkpoint_id: &str) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM checkpoints WHERE thread_id = ?1 AND checkpoint_id = ?2",
            params![thread_id, checkpoint_id],
        )?;
        Ok(())
    }

    /// Return thread ids that still have a checkpoint (need recovery on startup).
    pub fn list_threads_with_checkpoints(&self) -> Result<Vec<String>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare("SELECT DISTINCT thread_id FROM checkpoints")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
