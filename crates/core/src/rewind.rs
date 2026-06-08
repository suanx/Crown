//! Rewind (P2): roll a thread back to (and excluding) a message seq —
//! truncate the conversation and restore files to their pre-change content.
//!
//! Files are snapshotted before each write/edit via [`DbFileHistorySink`]
//! (injected into [`crate::engine::AgentEngine`]'s tool context). A rewind
//! restores each touched file to its EARLIEST recorded `before` at or after
//! the target seq, then truncates messages.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use deepseek_state::{Database, FileHistoryInsert, FileHistoryRepo, MessageRepo};
use deepseek_tools::FileHistorySink;

/// SQLite-backed [`FileHistorySink`] the engine injects into every tool
/// context. Records are best-effort: a failure logs and is swallowed so a
/// history-write hiccup never breaks the tool call itself.
pub struct DbFileHistorySink {
    db: Arc<Database>,
}

impl DbFileHistorySink {
    /// Construct from the shared database handle.
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl FileHistorySink for DbFileHistorySink {
    fn record(&self, thread_id: &str, message_seq: i64, path: &str, before: Option<String>) {
        let repo = FileHistoryRepo::new(self.db.as_ref());
        if let Err(e) = repo.record(FileHistoryInsert {
            thread_id: thread_id.into(),
            message_seq,
            path: path.into(),
            before,
        }) {
            tracing::warn!(error = %e, path, "file_history record failed");
        }
    }
}

/// Result of a rewind, for reporting back to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindOutcome {
    /// Number of messages deleted.
    pub messages_removed: usize,
    /// Number of distinct files restored (or deleted, if newly created).
    pub files_restored: usize,
}

/// Roll `thread_id` back to (and excluding) `message_seq`: restore files to
/// their earliest pre-change content recorded at/after `message_seq`, delete
/// those history rows, then truncate messages with `seq >= message_seq`.
///
/// File restoration is best-effort per file (an unwritable path logs and is
/// skipped); message truncation is authoritative.
pub async fn rewind_thread(
    db: Arc<Database>,
    thread_id: &str,
    message_seq: i64,
) -> Result<RewindOutcome> {
    // 1. Gather history at/after the target seq (newest first).
    let fh = FileHistoryRepo::new(db.as_ref());
    let rows = fh.since(thread_id, message_seq)?;

    // For each path, the EARLIEST `before` is the state to restore to. Rows
    // are DESC by id (newest first), so the LAST occurrence per path is the
    // oldest. Build path → oldest before.
    let mut oldest_before: HashMap<String, Option<String>> = HashMap::new();
    for row in &rows {
        // Insert-or-overwrite: since we iterate newest→oldest, the final
        // value left per path is the oldest occurrence.
        oldest_before.insert(row.path.clone(), row.before.clone());
    }

    let mut files_restored = 0usize;
    for (path, before) in &oldest_before {
        match before {
            Some(content) => {
                if let Err(e) = tokio::fs::write(path, content).await {
                    tracing::warn!(error = %e, path, "rewind: restore write failed");
                    continue;
                }
            }
            None => {
                // File didn't exist before — remove it (ignore if already gone).
                match tokio::fs::remove_file(path).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        tracing::warn!(error = %e, path, "rewind: restore delete failed");
                        continue;
                    }
                }
            }
        }
        files_restored += 1;
    }
    fh.delete_since(thread_id, message_seq)?;

    // 2. Truncate messages.
    let mrepo = MessageRepo::new(db.as_ref());
    let before_count = mrepo.count_for_thread(thread_id)? as usize;
    mrepo.truncate_after(thread_id, message_seq)?;
    let after_count = mrepo.count_for_thread(thread_id)? as usize;

    Ok(RewindOutcome {
        messages_removed: before_count.saturating_sub(after_count),
        files_restored,
    })
}
