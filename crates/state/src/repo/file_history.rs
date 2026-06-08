//! File edit history for rewind (P2).
//!
//! Every `write_file` / `edit_file` records the target file's content BEFORE
//! the change, attributed to the user-message `seq` that triggered the turn.
//! A rewind to a given seq restores all files changed at or after it.

use rusqlite::params;

use crate::db::{Database, DbError};

/// A file-history row: one pre-change snapshot.
#[derive(Debug, Clone)]
pub struct FileHistoryRow {
    /// Auto-increment id (also the chronological order key).
    pub id: i64,
    /// Owning thread.
    pub thread_id: String,
    /// User-message seq that triggered the turn this write happened in.
    pub message_seq: i64,
    /// Absolute file path that was about to change.
    pub path: String,
    /// File content before the change. `None` = the file did not exist.
    pub before: Option<String>,
    /// Insert time (unix ms).
    pub created_at: i64,
}

/// Insert payload for [`FileHistoryRepo::record`].
#[derive(Debug, Clone)]
pub struct FileHistoryInsert {
    /// Owning thread.
    pub thread_id: String,
    /// Triggering user-message seq.
    pub message_seq: i64,
    /// Absolute file path.
    pub path: String,
    /// Pre-change content (`None` = file did not exist).
    pub before: Option<String>,
}

/// Repository over the `file_history` table.
pub struct FileHistoryRepo<'a> {
    db: &'a Database,
}

impl<'a> FileHistoryRepo<'a> {
    /// Construct.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Record a pre-change snapshot.
    pub fn record(&self, input: FileHistoryInsert) -> Result<(), DbError> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO file_history (thread_id, message_seq, path, before, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.thread_id,
                input.message_seq,
                input.path,
                input.before,
                now
            ],
        )?;
        Ok(())
    }

    /// All history rows for a thread at or after `from_seq`, newest first
    /// (DESC by id). Restoring in this order, taking the OLDEST `before` per
    /// path, returns each file to its earliest pre-change state.
    pub fn since(&self, thread_id: &str, from_seq: i64) -> Result<Vec<FileHistoryRow>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, message_seq, path, before, created_at
             FROM file_history WHERE thread_id = ?1 AND message_seq >= ?2
             ORDER BY id DESC",
        )?;
        let rows = stmt
            .query_map(params![thread_id, from_seq], |r| {
                Ok(FileHistoryRow {
                    id: r.get(0)?,
                    thread_id: r.get(1)?,
                    message_seq: r.get(2)?,
                    path: r.get(3)?,
                    before: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Delete history rows at or after `from_seq` (after a rewind restores them).
    pub fn delete_since(&self, thread_id: &str, from_seq: i64) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM file_history WHERE thread_id = ?1 AND message_seq >= ?2",
            params![thread_id, from_seq],
        )?;
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
    fn record_and_since_newest_first() {
        let (d, _dir) = db();
        let repo = FileHistoryRepo::new(&d);
        repo.record(FileHistoryInsert {
            thread_id: "t".into(),
            message_seq: 1,
            path: "a.txt".into(),
            before: None,
        })
        .unwrap();
        repo.record(FileHistoryInsert {
            thread_id: "t".into(),
            message_seq: 2,
            path: "a.txt".into(),
            before: Some("v1".into()),
        })
        .unwrap();

        let rows = repo.since("t", 1).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].message_seq, 2, "newest first (DESC by id)");
        assert_eq!(rows[0].before.as_deref(), Some("v1"));

        // Only since seq>=2.
        assert_eq!(repo.since("t", 2).unwrap().len(), 1);

        repo.delete_since("t", 1).unwrap();
        assert_eq!(repo.since("t", 1).unwrap().len(), 0);
    }
}
