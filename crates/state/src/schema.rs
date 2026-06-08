//! SQLite schema migrations.
//!
//! Schema is embedded at compile time and applied idempotently on each open.
//! Additive column changes go through [`run_post_create_migrations`] which
//! checks `PRAGMA table_info(...)` before each ALTER so old DBs upgrade in
//! place without dropping data.

use rusqlite::Connection;

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Apply pragmas and run schema migrations.
pub(crate) fn init(conn: &Connection) -> rusqlite::Result<()> {
    // WAL mode + tuned pragmas (verified safe for our workload)
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -64_000_i64)?; // 64MB
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5_000_i64)?; // 5s

    conn.execute_batch(SCHEMA_SQL)?;
    run_post_create_migrations(conn)?;
    Ok(())
}

/// Idempotent additive-column migrations for DBs created before the column
/// existed in `schema.sql`. Each migration follows the same pattern:
///   1. PRAGMA table_info(<table>) to enumerate existing columns.
///   2. If the target column is missing, ALTER TABLE ADD COLUMN.
///
/// Adding a new migration:
///   - Append a new `if !column_exists(...)` block here.
///   - Mirror the column in `schema.sql` so fresh DBs skip this branch.
///   - Document the version range that introduced the migration.
fn run_post_create_migrations(conn: &Connection) -> rusqlite::Result<()> {
    // P3a (2026-05-28): threads.provider_id added for multi-provider support.
    // Defaults to 'deepseek' so pre-P3a rows behave correctly.
    if !column_exists(conn, "threads", "provider_id")? {
        conn.execute(
            "ALTER TABLE threads ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'deepseek'",
            [],
        )?;
    }

    // P3a task 4 (2026-05-28): usage table reshaped for provider-agnostic
    // cost tracking. Old shape used input_tokens / cache_hit_tokens; new
    // shape splits into cache_read / cache_miss / cache_creation / output.
    // Old column names stay (rusqlite has no DROP COLUMN before SQLite
    // 3.35) — they're just unused. Aggregations only read the new names.
    if !column_exists(conn, "usage", "cache_read_tokens")? {
        conn.execute(
            "ALTER TABLE usage ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !column_exists(conn, "usage", "cache_miss_tokens")? {
        conn.execute(
            "ALTER TABLE usage ADD COLUMN cache_miss_tokens INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !column_exists(conn, "usage", "cache_creation_tokens")? {
        conn.execute(
            "ALTER TABLE usage ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !column_exists(conn, "usage", "provider_id")? {
        conn.execute(
            "ALTER TABLE usage ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'deepseek'",
            [],
        )?;
    }
    if !column_exists(conn, "usage", "message_id")? {
        conn.execute("ALTER TABLE usage ADD COLUMN message_id TEXT", [])?;
    }

    // P4 (2026-05-30): threads.parent_thread_id for sub-agent threads.
    // NULL = top-level thread (the only kind shown in the sidebar).
    if !column_exists(conn, "threads", "parent_thread_id")? {
        conn.execute("ALTER TABLE threads ADD COLUMN parent_thread_id TEXT", [])?;
    }

    // P5 (2026-06-01): threads.project_id for persistent project grouping.
    if !column_exists(conn, "threads", "project_id")? {
        conn.execute("ALTER TABLE threads ADD COLUMN project_id TEXT", [])?;
    }

    // P5 (2026-06-02): per-thread reasoning effort for model requests.
    if !column_exists(conn, "threads", "thinking_effort")? {
        conn.execute(
            "ALTER TABLE threads ADD COLUMN thinking_effort TEXT NOT NULL DEFAULT 'medium'",
            [],
        )?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    // PRAGMA table_info is parameter-unfriendly (table name can't bind), so
    // we compose carefully — `table` here only ever comes from string
    // literals in this module, never user input.
    let sql = format!("PRAGMA table_info({})", table);
    let mut stmt = conn.prepare(&sql)?;
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cols.iter().any(|c| c == column))
}

#[cfg(test)]
mod tests {
    //! Test code intentionally uses `unwrap()` to fail loudly on schema
    //! errors — these tests assert on schema state and a panic with the
    //! sqlite error message is the most informative failure mode.
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// Creating an old-style threads table (no provider_id) and re-opening
    /// must add the column without losing rows.
    #[test]
    fn migration_adds_provider_id_to_legacy_table() {
        let conn = Connection::open_in_memory().unwrap();

        // Bootstrap full schema first so other tables (usage, messages, ...)
        // exist — the migration helper inspects all of them.
        conn.execute_batch(SCHEMA_SQL).unwrap();

        // Now simulate a pre-P3a `threads` shape by dropping the column.
        // SQLite 3.35+ supports DROP COLUMN; rusqlite ships modern SQLite.
        conn.execute("ALTER TABLE threads DROP COLUMN provider_id", [])
            .unwrap();

        // Insert a legacy row that predates the new column.
        conn.execute(
            "INSERT INTO threads (id, model, created_at, updated_at)
                VALUES ('legacy-thread', 'deepseek-v4-flash', 0, 0)",
            [],
        )
        .unwrap();

        // Confirm provider_id is gone before migration.
        assert!(!column_exists(&conn, "threads", "provider_id").unwrap());

        // Run migration (idempotent — second call is a no-op)
        run_post_create_migrations(&conn).unwrap();
        run_post_create_migrations(&conn).unwrap();

        // Column re-added; legacy row picked up the default.
        assert!(column_exists(&conn, "threads", "provider_id").unwrap());
        let provider: String = conn
            .query_row(
                "SELECT provider_id FROM threads WHERE id = 'legacy-thread'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(provider, "deepseek");
    }

    /// usage table also gets the provider-aware columns added when
    /// migrating from a pre-P3a-task-4 shape.
    #[test]
    fn migration_adds_usage_columns_to_legacy_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();

        // Drop the new columns to simulate a pre-task-4 schema.
        for col in [
            "cache_read_tokens",
            "cache_miss_tokens",
            "cache_creation_tokens",
            "provider_id",
            "message_id",
        ] {
            let sql = format!("ALTER TABLE usage DROP COLUMN {col}");
            conn.execute(&sql, []).unwrap();
        }

        for col in [
            "cache_read_tokens",
            "cache_miss_tokens",
            "cache_creation_tokens",
            "provider_id",
            "message_id",
        ] {
            assert!(
                !column_exists(&conn, "usage", col).unwrap(),
                "expected `{col}` to be missing pre-migration",
            );
        }

        run_post_create_migrations(&conn).unwrap();
        run_post_create_migrations(&conn).unwrap(); // idempotent

        for col in [
            "cache_read_tokens",
            "cache_miss_tokens",
            "cache_creation_tokens",
            "provider_id",
            "message_id",
        ] {
            assert!(
                column_exists(&conn, "usage", col).unwrap(),
                "expected `{col}` to exist after migration",
            );
        }
    }
}
