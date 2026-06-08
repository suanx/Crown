-- crates/state/src/schema.sql
CREATE TABLE IF NOT EXISTS threads (
    id              TEXT PRIMARY KEY,
    name            TEXT,
    model           TEXT NOT NULL,
    cwd             TEXT,
    permission_mode TEXT NOT NULL DEFAULT 'default',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    preview         TEXT,
    is_pinned       INTEGER NOT NULL DEFAULT 0,
    provider_id     TEXT NOT NULL DEFAULT 'deepseek',
    thinking_effort TEXT NOT NULL DEFAULT 'medium',
    parent_thread_id TEXT,
    project_id      TEXT REFERENCES projects(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_threads_updated_at ON threads(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_threads_status ON threads(status);

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    path        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_projects_updated_at ON projects(updated_at DESC);

CREATE TABLE IF NOT EXISTS messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id    TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq          INTEGER NOT NULL,
    role         TEXT NOT NULL,
    content_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    UNIQUE(thread_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_messages_thread_seq ON messages(thread_id, seq);

CREATE TABLE IF NOT EXISTS checkpoints (
    thread_id     TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    checkpoint_id TEXT NOT NULL DEFAULT 'latest',
    state_json    TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    PRIMARY KEY (thread_id, checkpoint_id)
);

CREATE TABLE IF NOT EXISTS usage (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id              TEXT REFERENCES threads(id) ON DELETE CASCADE,
    message_id             TEXT,
    provider_id            TEXT NOT NULL DEFAULT 'deepseek',
    model                  TEXT NOT NULL,
    cache_read_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_miss_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens          INTEGER NOT NULL DEFAULT 0,
    cost_usd               REAL NOT NULL DEFAULT 0,
    created_at             INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_thread ON usage(thread_id);
CREATE INDEX IF NOT EXISTS idx_usage_created_at ON usage(created_at DESC);

CREATE TABLE IF NOT EXISTS permission_rules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id    TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    source       TEXT NOT NULL,
    behavior     TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    rule_content TEXT,
    created_at   INTEGER NOT NULL,
    UNIQUE(thread_id, source, behavior, tool_name, rule_content)
);
CREATE INDEX IF NOT EXISTS idx_perm_rules_thread ON permission_rules(thread_id);

-- File edit history for rewind (P2). One row per write/edit, capturing the
-- file's content BEFORE the change so a rewind can restore it.
CREATE TABLE IF NOT EXISTS file_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id   TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    message_seq INTEGER NOT NULL,
    path        TEXT NOT NULL,
    before      TEXT,            -- NULL = file did not exist before
    created_at  INTEGER NOT NULL
);
-- Tool dispatch trace (Codex-aligned). Persisted per-turn summary so the
-- diagnostics UI and post-hoc analysis can read tool success/failure/abort
-- counts without replaying the full message log.
CREATE TABLE IF NOT EXISTS tool_dispatch_trace (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id   TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    message_id  TEXT NOT NULL,
    total       INTEGER NOT NULL DEFAULT 0,
    success     INTEGER NOT NULL DEFAULT 0,
    recoverable INTEGER NOT NULL DEFAULT 0,
    error       INTEGER NOT NULL DEFAULT 0,
    aborted     INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    categories  TEXT,            -- comma-separated failure categories seen
    subgoals    TEXT,            -- pipe-separated active todo labels
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tool_trace_thread ON tool_dispatch_trace(thread_id);
CREATE INDEX IF NOT EXISTS idx_tool_trace_message ON tool_dispatch_trace(message_id);
