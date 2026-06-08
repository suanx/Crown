//! Rewind commands (P2): roll a thread back to a user message, restoring
//! files and truncating the conversation.

use deepseek_state::{FileHistoryRepo, MessageRepo};

use crate::dto::{RewindPointDto, ThreadDto};
use crate::AppState;

/// Rewind a thread to (and excluding) `message_seq`: restore files changed
/// since, truncate messages, evict the in-memory cache so the next load
/// rebuilds from the truncated history. Returns the fresh thread payload.
#[tauri::command]
pub async fn rewind_thread(
    state: tauri::State<'_, AppState>,
    thread_id: String,
    message_seq: i64,
) -> Result<ThreadDto, String> {
    deepseek_core::rewind::rewind_thread(state.db.clone(), &thread_id, message_seq)
        .await
        .map_err(|e| e.to_string())?;
    // Drop cached in-memory state so the next get_or_load rebuilds the log
    // from the now-truncated messages table.
    state.engine.cache().remove(&thread_id);
    super::threads::get_thread(state, thread_id).await
}

/// List the points a thread can be rewound to (one per user message), with a
/// preview and how many files changed at or after each point.
#[tauri::command]
pub async fn list_rewind_points(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<Vec<RewindPointDto>, String> {
    let mrepo = MessageRepo::new(state.db.as_ref());
    let fhrepo = FileHistoryRepo::new(state.db.as_ref());
    let messages = mrepo
        .load_by_thread(&thread_id)
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for m in messages {
        if m.role != "user" {
            continue;
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&m.content_json).unwrap_or(serde_json::Value::Null);
        let raw = parsed.get("content").and_then(|c| c.as_str()).unwrap_or("");
        // Strip a leading slash-command system-reminder for a clean preview.
        let cleaned = strip_reminder(raw);
        let preview: String = cleaned.chars().take(80).collect();

        let files_changed = fhrepo
            .since(&thread_id, m.seq)
            .map(|rows| {
                let mut paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
                for r in &rows {
                    paths.insert(r.path.as_str());
                }
                paths.len() as u64
            })
            .unwrap_or(0);

        out.push(RewindPointDto {
            message_seq: m.seq,
            preview,
            files_changed,
        });
    }
    Ok(out)
}

/// Strip a leading `<system-reminder>...</system-reminder>` block (slash
/// command injection) for a clean rewind-point preview.
fn strip_reminder(s: &str) -> &str {
    let trimmed = s.trim_start();
    if let Some(rest) = trimmed.strip_prefix("<system-reminder>") {
        if let Some(end) = rest.find("</system-reminder>") {
            return rest[end + "</system-reminder>".len()..].trim_start();
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_reminder_removes_block() {
        let s = "<system-reminder>\nplan stuff\n</system-reminder>\n\nreal task";
        assert_eq!(strip_reminder(s), "real task");
    }

    #[test]
    fn strip_reminder_passthrough_plain() {
        assert_eq!(strip_reminder("just text"), "just text");
    }
}
