//! Usage repository: per-turn token + cost persistence and aggregation.
//!
//! Each completed turn writes one row via [`UsageRepo::insert`]. Aggregations
//! ([`UsageRepo::total_since`], [`UsageRepo::thread_cost`],
//! [`UsageRepo::cache_savings_since`]) feed the cost UI:
//!
//! - `total_since` powers the rolling [`UsageStatsWindow`] panel
//! - `thread_cost` powers the per-thread `ThreadDto.cost_usd` field
//! - `cache_savings_since` powers the "你省了多少 $" UI badge
//!
//! All rows are append-only; nothing in the cost-tracking pipeline ever
//! updates or deletes a usage row outside of `ON DELETE CASCADE` cleanup
//! when its parent thread is hard-deleted.

use rusqlite::params;

use crate::db::{Database, DbError};

/// Insert payload for a single completed turn.
#[derive(Debug, Clone)]
pub struct UsageInsert {
    /// Thread the turn belongs to.
    pub thread_id: String,
    /// Assistant message id produced by this turn.
    pub message_id: String,
    /// Provider identifier (e.g. `"deepseek"`).
    pub provider_id: String,
    /// Model id (e.g. `"deepseek-v4-flash"`).
    pub model: String,
    /// Cache-read input tokens (cheapest tier).
    pub cache_read_tokens: u64,
    /// Uncached input tokens.
    pub cache_miss_tokens: u64,
    /// Cache-creation input tokens (Anthropic-only; DeepSeek = 0).
    pub cache_creation_tokens: u64,
    /// Output / completion tokens.
    pub output_tokens: u64,
    /// Cost in USD computed at insert time using the active price table.
    pub cost_usd: f64,
    /// Timestamp in epoch ms.
    pub created_at: i64,
}

/// Aggregate summary for a time window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageAggregate {
    /// Sum of cache-read input tokens.
    pub cache_read_tokens: u64,
    /// Sum of uncached input tokens.
    pub cache_miss_tokens: u64,
    /// Sum of cache-creation input tokens.
    pub cache_creation_tokens: u64,
    /// Sum of output tokens.
    pub output_tokens: u64,
    /// Sum of cost in USD.
    pub total_cost_usd: f64,
    /// Sum of cache-read tokens that fell on a non-zero (miss − read) gap,
    /// converted into USD savings using the model row's effective price
    /// table. P3a recomputes from current prices for simplicity; if prices
    /// shift, historical savings drift — accepted trade-off (see plan).
    pub cumulative_cache_saved_usd: f64,
}

/// Usage repository.
pub struct UsageRepo<'a> {
    db: &'a Database,
}

impl<'a> UsageRepo<'a> {
    /// Create a new repository handle. Cheap.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Append one turn's usage row. Idempotent only at the application level
    /// (caller chooses to retry or not); SQL has no UNIQUE constraint on
    /// `message_id` because retry semantics differ across error classes.
    pub fn insert(&self, u: UsageInsert) -> Result<(), DbError> {
        // Token counts are u64 but SQLite INTEGER is i64. Saturate rather than
        // silently wrap to a negative value if a (practically impossible) count
        // exceeds i64::MAX — keeps stored counts monotonic and non-negative.
        let clamp = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO usage (
                thread_id, message_id, provider_id, model,
                cache_read_tokens, cache_miss_tokens, cache_creation_tokens,
                output_tokens, cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                u.thread_id,
                u.message_id,
                u.provider_id,
                u.model,
                clamp(u.cache_read_tokens),
                clamp(u.cache_miss_tokens),
                clamp(u.cache_creation_tokens),
                clamp(u.output_tokens),
                u.cost_usd,
                u.created_at,
            ],
        )?;
        Ok(())
    }

    /// Aggregate cost + tokens since the given epoch ms (inclusive).
    /// Pass `0` for lifetime-wide totals.
    ///
    /// `cumulative_cache_saved_usd` is computed by joining usage rows with
    /// the live pricing table at the call site — this repo only returns the
    /// raw token sums; the engine layer converts to USD because the price
    /// table lives in `crates/core::pricing` and `crates/state` deliberately
    /// has no `core` dependency. P3a task 6 wires the conversion.
    pub fn total_since(&self, since_ms: i64) -> Result<UsageAggregate, DbError> {
        let conn = self.db.conn();
        let agg = conn.query_row(
            "SELECT COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_miss_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0)
             FROM usage WHERE created_at >= ?1",
            params![since_ms],
            |r| {
                Ok(UsageAggregate {
                    cache_read_tokens: r.get::<_, i64>(0)? as u64,
                    cache_miss_tokens: r.get::<_, i64>(1)? as u64,
                    cache_creation_tokens: r.get::<_, i64>(2)? as u64,
                    output_tokens: r.get::<_, i64>(3)? as u64,
                    total_cost_usd: r.get(4)?,
                    cumulative_cache_saved_usd: 0.0,
                })
            },
        )?;
        Ok(agg)
    }

    /// Sum cost for one thread's lifetime (all rows). Powers
    /// `ThreadDto.cost_usd`.
    pub fn thread_cost(&self, thread_id: &str) -> Result<f64, DbError> {
        let conn = self.db.conn();
        let cost: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage WHERE thread_id = ?1",
            params![thread_id],
            |r| r.get(0),
        )?;
        Ok(cost)
    }

    /// Per-(provider, model) cache-read token sums in the window. The caller
    /// joins with the active price table to convert to USD savings — see
    /// `total_since`'s rationale for why USD computation lives outside this
    /// crate.
    pub fn cache_read_breakdown_since(
        &self,
        since_ms: i64,
    ) -> Result<Vec<CacheReadBreakdownRow>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT provider_id, model, COALESCE(SUM(cache_read_tokens), 0)
             FROM usage
             WHERE created_at >= ?1
             GROUP BY provider_id, model",
        )?;
        let rows = stmt
            .query_map(params![since_ms], |r| {
                Ok(CacheReadBreakdownRow {
                    provider_id: r.get(0)?,
                    model: r.get(1)?,
                    cache_read_tokens: r.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Daily-aggregated token + cost breakdown for a time window.
    /// Returns one row per calendar day (UTC), ordered ascending.
    /// Useful for chart rendering in the billing UI.
    pub fn daily_breakdown_since(&self, since_ms: i64) -> Result<Vec<DailyUsageRow>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT created_at / 86400000 AS day,
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_miss_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0)
             FROM usage
             WHERE created_at >= ?1
             GROUP BY day
             ORDER BY day ASC",
        )?;
        let rows = stmt
            .query_map(params![since_ms], |r| {
                Ok(DailyUsageRow {
                    day_epoch_ms: r.get::<_, i64>(0)? * 86400000,
                    cache_read_tokens: r.get::<_, i64>(1)? as u64,
                    cache_miss_tokens: r.get::<_, i64>(2)? as u64,
                    output_tokens: r.get::<_, i64>(3)? as u64,
                    total_cost_usd: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

/// A single day aggregation for chart rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyUsageRow {
    /// Start-of-day epoch ms (UTC).
    pub day_epoch_ms: i64,
    pub cache_read_tokens: u64,
    pub cache_miss_tokens: u64,
    pub output_tokens: u64,
    pub total_cost_usd: f64,
}


/// Per-(provider, model) cache-read token row used by
/// [`UsageRepo::cache_read_breakdown_since`].
#[derive(Debug, Clone, PartialEq)]
pub struct CacheReadBreakdownRow {
    /// Provider id (e.g. `"deepseek"`).
    pub provider_id: String,
    /// Model id (e.g. `"deepseek-v4-flash"`).
    pub model: String,
    /// Cache-read tokens summed across rows for this (provider, model).
    pub cache_read_tokens: u64,
}
