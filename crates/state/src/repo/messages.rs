//! Message repository: append-only.

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::{Database, DbError};

/// Message row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    /// Auto-increment id.
    pub id: i64,
    /// Owning thread.
    pub thread_id: String,
    /// Sequence within thread (0-based).
    pub seq: i64,
    /// `user | assistant | system | tool`.
    pub role: String,
    /// Full ChatMessage JSON (content + reasoning + tool_calls + tool_call_id).
    pub content_json: String,
    /// Insert time (unix ms).
    pub created_at: i64,
}

/// Insert payload.
#[derive(Debug, Clone)]
pub struct MessageInsert {
    /// Thread to append into.
    pub thread_id: String,
    /// Caller-provided sequence number. Must be unique per thread.
    pub seq: i64,
    /// Role.
    pub role: String,
    /// Serialized ChatMessage.
    pub content_json: String,
}

/// Message repository.
pub struct MessageRepo<'a> {
    db: &'a Database,
}

impl<'a> MessageRepo<'a> {
    /// Construct.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Append a message. Errors on duplicate (thread_id, seq).
    pub fn append(&self, input: MessageInsert) -> Result<MessageRow, DbError> {
        let now = Utc::now().timestamp_millis();
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO messages (thread_id, seq, role, content_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.thread_id,
                input.seq,
                input.role,
                input.content_json,
                now
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok(MessageRow {
            id,
            thread_id: input.thread_id,
            seq: input.seq,
            role: input.role,
            content_json: input.content_json,
            created_at: now,
        })
    }

    /// Append a message, assigning the next sequence number **atomically**
    /// inside a single transaction (`MAX(seq)+1` + INSERT). This prevents the
    /// (thread_id, seq) UNIQUE collision that a separate `max_seq()` +
    /// `append()` can hit when two persists race on the same thread
    /// (BUG-E2E-003 — surfaced by concurrent sub-agent + main persists).
    pub fn append_next(
        &self,
        thread_id: &str,
        role: &str,
        content_json: &str,
    ) -> Result<MessageRow, DbError> {
        let now = Utc::now().timestamp_millis();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let next_seq: i64 = {
            let n: Option<i64> = tx.query_row(
                "SELECT MAX(seq) FROM messages WHERE thread_id = ?1",
                params![thread_id],
                |row| row.get(0),
            )?;
            n.unwrap_or(-1) + 1
        };
        tx.execute(
            "INSERT INTO messages (thread_id, seq, role, content_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![thread_id, next_seq, role, content_json, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(MessageRow {
            id,
            thread_id: thread_id.to_string(),
            seq: next_seq,
            role: role.to_string(),
            content_json: content_json.to_string(),
            created_at: now,
        })
    }

    /// Load all messages for a thread, ordered by seq ASC.
    pub fn load_by_thread(&self, thread_id: &str) -> Result<Vec<MessageRow>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, seq, role, content_json, created_at
             FROM messages WHERE thread_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![thread_id], |row| {
                Ok(MessageRow {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    seq: row.get(2)?,
                    role: row.get(3)?,
                    content_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Count messages in a thread.
    pub fn count_for_thread(&self, thread_id: &str) -> Result<u64, DbError> {
        let conn = self.db.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE thread_id = ?1",
            params![thread_id],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    /// Highest seq for a thread (for resume / append). Returns -1 if no messages yet.
    pub fn max_seq(&self, thread_id: &str) -> Result<i64, DbError> {
        let conn = self.db.conn();
        let n: Option<i64> = conn.query_row(
            "SELECT MAX(seq) FROM messages WHERE thread_id = ?1",
            params![thread_id],
            |row| row.get(0),
        )?;
        Ok(n.unwrap_or(-1))
    }

    /// Delete all messages with `seq >= from_seq` for a thread. Used by
    /// rewind to roll the conversation back to (and excluding) a message.
    pub fn truncate_after(&self, thread_id: &str, from_seq: i64) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM messages WHERE thread_id = ?1 AND seq >= ?2",
            params![thread_id, from_seq],
        )?;
        Ok(())
    }

    /// Atomically replace **all** messages for `thread_id` with
    /// `replacement` (role, content_json pairs), assigning fresh 0-based
    /// contiguous sequence numbers.
    ///
    /// This is the persistence side of context compaction (fold): the
    /// in-memory log is rewritten to `[summary, ...recent_tail]`, and this
    /// mirrors that to disk so a later cache reload (LRU eviction / restart)
    /// doesn't resurrect the elided history. Runs in a single transaction —
    /// either the whole rewrite lands or none of it does, so a crash
    /// mid-rewrite can never leave a half-folded thread.
    pub fn rewrite_thread(
        &self,
        thread_id: &str,
        replacement: Vec<(String, String)>,
    ) -> Result<(), DbError> {
        let now = Utc::now().timestamp_millis();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM messages WHERE thread_id = ?1",
            params![thread_id],
        )?;
        for (seq, (role, content_json)) in replacement.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO messages (thread_id, seq, role, content_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![thread_id, seq as i64, role, content_json, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Database;

    fn db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let d = Database::open(dir.path().join("state.db")).unwrap();
        d.conn()
            .execute(
                "INSERT INTO threads (id, model, created_at, updated_at) VALUES ('t','m',0,0)",
                [],
            )
            .unwrap();
        (d, dir)
    }

    #[test]
    fn truncate_after_removes_seq_and_beyond() {
        let (d, _dir) = db();
        let repo = MessageRepo::new(&d);
        for seq in 0..5 {
            repo.append(MessageInsert {
                thread_id: "t".into(),
                seq,
                role: "user".into(),
                content_json: format!("{{\"seq\":{seq}}}"),
            })
            .unwrap();
        }
        repo.truncate_after("t", 2).unwrap();
        let rows = repo.load_by_thread("t").unwrap();
        assert_eq!(rows.len(), 2, "seq 0,1 kept; 2,3,4 removed");
        assert_eq!(rows.last().unwrap().seq, 1);
    }
}
