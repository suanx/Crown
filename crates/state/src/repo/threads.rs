//! Thread repository: CRUD + list + search.

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::db::{Database, DbError};

/// Thread row (full).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Thread ID (ULID).
    pub id: String,
    /// User-set or auto-derived title.
    pub name: Option<String>,
    /// Model identifier (e.g. "deepseek-v4-flash").
    pub model: String,
    /// Working directory.
    pub cwd: Option<String>,
    /// Permission mode (one of `default | plan | acceptEdits | bypassPermissions | dontAsk`).
    pub permission_mode: String,
    /// Creation timestamp (unix ms).
    pub created_at: i64,
    /// Last activity timestamp (unix ms).
    pub updated_at: i64,
    /// `active` or `archived`.
    pub status: String,
    /// Last message preview (truncated).
    pub preview: Option<String>,
    /// Whether user pinned this thread.
    pub is_pinned: bool,
    /// Provider id (e.g. `deepseek`). Defaults to `deepseek` for rows
    /// created before P3a; future providers (openai/anthropic/openrouter)
    /// set this explicitly at create time.
    pub provider_id: String,
    /// 每个线程独立的推理强度：`low | medium | high | ultra`。
    pub thinking_effort: String,
    /// Parent thread id when this is a sub-agent thread (P4). `None` for
    /// top-level threads (the only ones shown in the sidebar).
    pub parent_thread_id: Option<String>,
    /// 所属项目 ID；为空时显示在“无项目”。
    pub project_id: Option<String>,
}

/// Lightweight thread summary for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    /// Thread ID.
    pub id: String,
    /// Display name.
    pub name: Option<String>,
    /// Last activity (unix ms).
    pub updated_at: i64,
    /// Last message preview.
    pub preview: Option<String>,
    /// Whether pinned.
    pub is_pinned: bool,
    /// Message count for the badge.
    pub message_count: u64,
    /// Provider id (e.g. "deepseek").
    pub provider_id: String,
    /// 所属项目 ID；为空时显示在“无项目”。
    pub project_id: Option<String>,
}

/// Insert payload.
#[derive(Debug, Default, Clone)]
pub struct ThreadInsert {
    /// Initial name; if `None`, derived from first user message later.
    pub name: Option<String>,
    /// Model id.
    pub model: String,
    /// Working directory.
    pub cwd: Option<String>,
    /// Initial permission mode.
    pub permission_mode: String,
    /// Provider id; empty defaults to `deepseek`.
    pub provider_id: String,
    /// 初始推理强度。
    pub thinking_effort: Option<String>,
    /// Parent thread id for sub-agent threads (P4). `None` = top-level.
    pub parent_thread_id: Option<String>,
    /// 所属项目 ID。
    pub project_id: Option<String>,
}

/// Update payload. `None` means "leave unchanged"; `Some(None)` means "set NULL".
#[derive(Debug, Default, Clone)]
pub struct ThreadUpdate {
    /// New name (or clear).
    pub name: Option<Option<String>>,
    /// New permission mode.
    pub permission_mode: Option<String>,
    /// New cwd.
    pub cwd: Option<Option<String>>,
    /// New preview.
    pub preview: Option<Option<String>>,
    /// Pin/unpin.
    pub is_pinned: Option<bool>,
    /// Bump `updated_at` to now even if no other field changed.
    pub touch: bool,
    /// New model.
    pub model: Option<String>,
    /// New provider id (rare; usually only set at creation).
    pub provider_id: Option<String>,
    /// 新推理强度。
    pub thinking_effort: Option<String>,
    /// 新项目 ID；`Some(None)` 表示移出项目。
    pub project_id: Option<Option<String>>,
}

/// Thread repository.
pub struct ThreadRepo<'a> {
    db: &'a Database,
}

impl<'a> ThreadRepo<'a> {
    /// Create a new repository handle. Cheap.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new thread. Returns the created row.
    pub fn create(&self, input: ThreadInsert) -> Result<Thread, DbError> {
        let id = Ulid::new().to_string();
        let now = Utc::now().timestamp_millis();
        let thread = Thread {
            id,
            name: input.name,
            model: if input.model.is_empty() {
                "deepseek-v4-flash".into()
            } else {
                input.model
            },
            cwd: input.cwd,
            permission_mode: if input.permission_mode.is_empty() {
                "default".into()
            } else {
                input.permission_mode
            },
            created_at: now,
            updated_at: now,
            status: "active".into(),
            preview: None,
            is_pinned: false,
            provider_id: if input.provider_id.is_empty() {
                "deepseek".into()
            } else {
                input.provider_id
            },
            thinking_effort: input
                .thinking_effort
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "medium".into()),
            parent_thread_id: input.parent_thread_id,
            project_id: input.project_id,
        };
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO threads (id, name, model, cwd, permission_mode, created_at, updated_at, status, preview, is_pinned, provider_id, thinking_effort, parent_thread_id, project_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                thread.id,
                thread.name,
                thread.model,
                thread.cwd,
                thread.permission_mode,
                thread.created_at,
                thread.updated_at,
                thread.status,
                thread.preview,
                thread.is_pinned as i64,
                thread.provider_id,
                thread.thinking_effort,
                thread.parent_thread_id,
                thread.project_id,
            ],
        )?;
        Ok(thread)
    }

    /// Get a thread by id.
    pub fn get(&self, id: &str) -> Result<Thread, DbError> {
        let conn = self.db.conn();
        conn.query_row(
            "SELECT id, name, model, cwd, permission_mode, created_at, updated_at, status, preview, is_pinned, provider_id, thinking_effort, parent_thread_id, project_id
             FROM threads WHERE id = ?1",
            params![id],
            row_to_thread,
        )
        .map_err(Into::into)
    }

    /// List all non-archived **top-level** threads (sub-agent threads are
    /// hidden) ordered by pinned DESC then updated_at DESC.
    pub fn list(&self) -> Result<Vec<ThreadSummary>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.updated_at, t.preview, t.is_pinned, t.provider_id, t.project_id,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id = t.id) AS msg_count
             FROM threads t
             WHERE t.status = 'active' AND t.parent_thread_id IS NULL
             ORDER BY t.is_pinned DESC, t.updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ThreadSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                    preview: row.get(3)?,
                    is_pinned: row.get::<_, i64>(4)? != 0,
                    provider_id: row.get(5)?,
                    project_id: row.get(6)?,
                    message_count: row.get::<_, i64>(7)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// LIKE `%query%` over name + preview. Case-insensitive.
    pub fn search(&self, query: &str) -> Result<Vec<ThreadSummary>, DbError> {
        let pattern = format!("%{}%", query.to_lowercase());
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.updated_at, t.preview, t.is_pinned, t.provider_id, t.project_id,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id = t.id)
             FROM threads t
             WHERE t.status = 'active'
               AND (LOWER(COALESCE(t.name, '')) LIKE ?1 OR LOWER(COALESCE(t.preview, '')) LIKE ?1)
             ORDER BY t.is_pinned DESC, t.updated_at DESC",
        )?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(ThreadSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                    preview: row.get(3)?,
                    is_pinned: row.get::<_, i64>(4)? != 0,
                    provider_id: row.get(5)?,
                    project_id: row.get(6)?,
                    message_count: row.get::<_, i64>(7)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Apply update fields. Always bumps `updated_at` if anything changed or if `touch` is set.
    pub fn update(&self, id: &str, upd: ThreadUpdate) -> Result<(), DbError> {
        let now = Utc::now().timestamp_millis();
        let mut sets: Vec<&'static str> = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(name) = upd.name {
            sets.push("name = ?");
            params_vec.push(Box::new(name));
        }
        if let Some(mode) = upd.permission_mode {
            sets.push("permission_mode = ?");
            params_vec.push(Box::new(mode));
        }
        if let Some(cwd) = upd.cwd {
            sets.push("cwd = ?");
            params_vec.push(Box::new(cwd));
        }
        if let Some(preview) = upd.preview {
            sets.push("preview = ?");
            params_vec.push(Box::new(preview));
        }
        if let Some(pinned) = upd.is_pinned {
            sets.push("is_pinned = ?");
            params_vec.push(Box::new(pinned as i64));
        }
        if let Some(model) = upd.model {
            sets.push("model = ?");
            params_vec.push(Box::new(model));
        }
        if let Some(provider_id) = upd.provider_id {
            sets.push("provider_id = ?");
            params_vec.push(Box::new(provider_id));
        }
        if let Some(thinking_effort) = upd.thinking_effort {
            sets.push("thinking_effort = ?");
            params_vec.push(Box::new(thinking_effort));
        }
        if let Some(project_id) = upd.project_id {
            sets.push("project_id = ?");
            params_vec.push(Box::new(project_id));
        }
        if sets.is_empty() && !upd.touch {
            return Ok(());
        }
        sets.push("updated_at = ?");
        params_vec.push(Box::new(now));
        let sql = format!("UPDATE threads SET {} WHERE id = ?", sets.join(", "));
        params_vec.push(Box::new(id.to_string()));
        let conn = self.db.conn();
        let refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
        Ok(())
    }

    /// Hard delete. ON DELETE CASCADE removes messages / checkpoints / usage.
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute("DELETE FROM threads WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Mark archived (soft delete; not currently exposed via UI).
    pub fn archive(&self, id: &str) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE threads SET status = 'archived', updated_at = ?2 WHERE id = ?1",
            params![id, Utc::now().timestamp_millis()],
        )?;
        Ok(())
    }
}

fn row_to_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: row.get(0)?,
        name: row.get(1)?,
        model: row.get(2)?,
        cwd: row.get(3)?,
        permission_mode: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        status: row.get(7)?,
        preview: row.get(8)?,
        is_pinned: row.get::<_, i64>(9)? != 0,
        provider_id: row.get(10)?,
        thinking_effort: row.get(11)?,
        parent_thread_id: row.get(12)?,
        project_id: row.get(13)?,
    })
}
