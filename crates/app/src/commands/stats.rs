//! Stats / diagnostics commands.
//!
//! `get_usage_stats` aggregates the [`deepseek_state::UsageRepo`] over a
//! [`crate::dto::UsageStatsWindow`] window and converts cache-read tokens
//! into USD savings via [`deepseek_core::pricing::cache_savings_usd`].
//!
//! `export_diagnostics` is still a P5+ placeholder.

use deepseek_core::pricing::{self, ProviderId};
use deepseek_state::UsageRepo;

use crate::dto::{GetUsageStatsInput, UsageStatsDto, UsageStatsWindow};
use crate::AppState;

/// Aggregate token + cost totals across the requested window.
///
/// Defaults to [`UsageStatsWindow::Session`] when `input` is absent or
/// `window` is `None`.
#[tauri::command]
pub async fn get_usage_stats(
    state: tauri::State<'_, AppState>,
    input: Option<GetUsageStatsInput>,
) -> Result<UsageStatsDto, String> {
    let window = input
        .and_then(|i| i.window)
        .unwrap_or(UsageStatsWindow::Session);
    let since_ms = window.since_ms(state.session_start_ms);

    let urepo = UsageRepo::new(state.db.as_ref());
    let agg = urepo.total_since(since_ms).map_err(|e| e.to_string())?;

    // Cache-savings rollup: for each (provider, model) bucket in the
    // window, look up the matching pricing record and convert the
    // cache-read tokens into the USD it would have cost at the cache-miss
    // tier. Sum across buckets.
    //
    // Computing "live" with current pricing means historical savings will
    // drift if prices change — accepted trade-off (avoids per-row price
    // snapshots in the usage table). See P3a plan task 4 rationale.
    let mut cumulative_cache_saved_usd = 0.0;
    match urepo.cache_read_breakdown_since(since_ms) {
        Ok(rows) => {
            for r in rows {
                let provider = ProviderId::from_str_lossy(&r.provider_id);
                cumulative_cache_saved_usd +=
                    pricing::cache_savings_usd(provider, &r.model, r.cache_read_tokens);
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "cache_read_breakdown lookup failed; cache savings = 0");
        }
    }

    let total_input_tokens = agg.cache_read_tokens + agg.cache_miss_tokens;
    let cache_hit_ratio = if total_input_tokens == 0 {
        0.0
    } else {
        agg.cache_read_tokens as f64 / total_input_tokens as f64
    };

    Ok(UsageStatsDto {
        total_cost_usd: agg.total_cost_usd,
        cumulative_cache_saved_usd,
        cache_read_tokens: agg.cache_read_tokens,
        cache_miss_tokens: agg.cache_miss_tokens,
        cache_creation_tokens: agg.cache_creation_tokens,
        output_tokens: agg.output_tokens,
        cache_hit_ratio,
        window_label: window.as_str().into(),
        // Budget mode is intentionally out of P3a scope; both fields stay
        // None until the user opts into a budget feature in a later phase.
        budget_limit_usd: None,
        budget_used_pct: None,
    })
}

/// Export a diagnostics snapshot as a pretty-printed JSON string for bug
/// reports: app/build info, platform, lifetime usage rollup, and store
/// counts. Contains no secrets (API keys are never included).
#[tauri::command]
pub async fn export_diagnostics(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let urepo = UsageRepo::new(state.db.as_ref());
    // Lifetime usage (since epoch 0).
    let lifetime = urepo.total_since(0).map_err(|e| e.to_string())?;

    let thread_count = deepseek_state::ThreadRepo::new(state.db.as_ref())
        .list()
        .map(|v| v.len())
        .unwrap_or(0);
    let mcp_servers = state.mcp.list_servers().len();

    let diag = serde_json::json!({
        "app": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        },
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "usage_lifetime": {
            "total_cost_usd": lifetime.total_cost_usd,
            "cache_read_tokens": lifetime.cache_read_tokens,
            "cache_miss_tokens": lifetime.cache_miss_tokens,
            "cache_creation_tokens": lifetime.cache_creation_tokens,
            "output_tokens": lifetime.output_tokens,
        },
        "store": {
            "thread_count": thread_count,
            "mcp_server_count": mcp_servers,
        },
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    serde_json::to_string_pretty(&diag).map_err(|e| e.to_string())
}
