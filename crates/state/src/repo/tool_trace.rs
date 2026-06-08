//! Tool dispatch trace repository.
//!
//! Persists per-turn [`ToolDispatchSummary`] rows to SQLite so the
//! diagnostics UI and post-hoc analysis can query tool performance
//! without replaying the full conversation log. Codex-aligned:
//! mirrors `codex-rs/core/src/tools/tool_dispatch_trace.rs`.

use rusqlite::params;

use crate::db::{Database, DbError};

/// Insert payload for [`ToolDispatchTraceRepo::insert`].
#[derive(Debug, Clone)]
pub struct ToolDispatchTraceInsert {
    /// Owning thread.
    pub thread_id: String,
    /// Assistant message id this turn's dispatch trace belongs to.
    pub message_id: String,
    /// Total tool calls dispatched this turn.
    pub total: usize,
    /// Successful tool calls.
    pub success: usize,
    /// Tools that failed but were recoverable (non-zero exit, etc.).
    pub recoverable: usize,
    /// Hard errors (tool unavailable, permission denied, parse failure).
    pub error: usize,
    /// Tools aborted by user cancellation or sibling abort.
    pub aborted: usize,
    /// Cumulative wall-clock time across all tool calls in ms.
    pub total_duration_ms: u64,
    /// Comma-separated failure categories seen this turn.
    pub categories: Option<String>,
    /// Pipe-separated active subgoal labels during this turn.
    pub subgoals: Option<String>,
}

/// A trace row loaded from the database.
#[derive(Debug, Clone)]
pub struct ToolDispatchTraceRow {
    /// Auto-increment primary key.
    pub id: i64,
    /// Owning thread id.
    pub thread_id: String,
    /// Assistant message id this turn's dispatch trace belongs to.
    pub message_id: String,
    /// Total tool calls dispatched this turn.
    pub total: usize,
    /// Successful tool calls.
    pub success: usize,
    /// Tools that failed but were recoverable.
    pub recoverable: usize,
    /// Hard errors.
    pub error: usize,
    /// Tools aborted.
    pub aborted: usize,
    /// Cumulative wall-clock time across all tool calls in ms.
    pub total_duration_ms: u64,
    /// Comma-separated failure categories seen this turn.
    pub categories: Option<String>,
    /// Pipe-separated active subgoal labels during this turn.
    pub subgoals: Option<String>,
    /// Insertion time (unix ms).
    pub created_at: i64,
}

/// Repository over the `tool_dispatch_trace` table.
pub struct ToolDispatchTraceRepo<'a> {
    db: &'a Database,
}

impl<'a> ToolDispatchTraceRepo<'a> {
    /// Construct a new repo from a database reference.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Persist a turn's tool dispatch summary.
    pub fn insert(&self, input: ToolDispatchTraceInsert) -> Result<(), DbError> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO tool_dispatch_trace
                (thread_id, message_id, total, success, recoverable, error, aborted,
                 total_duration_ms, categories, subgoals, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.thread_id,
                input.message_id,
                input.total as i64,
                input.success as i64,
                input.recoverable as i64,
                input.error as i64,
                input.aborted as i64,
                input.total_duration_ms as i64,
                input.categories,
                input.subgoals,
                now,
            ],
        )?;
        Ok(())
    }

    /// Load all trace rows for a thread, most recent first.
    pub fn for_thread(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<ToolDispatchTraceRow>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, message_id, total, success, recoverable, error, aborted,
                    total_duration_ms, categories, subgoals, created_at
             FROM tool_dispatch_trace
             WHERE thread_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![thread_id, limit as i64], |r| {
                Ok(ToolDispatchTraceRow {
                    id: r.get(0)?,
                    thread_id: r.get(1)?,
                    message_id: r.get(2)?,
                    total: r.get::<_, i64>(3)? as usize,
                    success: r.get::<_, i64>(4)? as usize,
                    recoverable: r.get::<_, i64>(5)? as usize,
                    error: r.get::<_, i64>(6)? as usize,
                    aborted: r.get::<_, i64>(7)? as usize,
                    total_duration_ms: r.get::<_, i64>(8)? as u64,
                    categories: r.get(9)?,
                    subgoals: r.get(10)?,
                    created_at: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
