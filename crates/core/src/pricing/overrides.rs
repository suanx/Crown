//! User-level pricing overrides.
//!
//! Loaded from `<config_dir>/crown/config.toml` with the schema:
//!
//! ```toml
//! [providers.openrouter]
//! # Provider-level metadata (extension point — base_url etc. would
//! # land here once we ship multi-provider switching)
//!
//! [[providers.openrouter.pricing]]
//! model = "deepseek/deepseek-v4-flash"
//! cache_read_per_m_usd = 0.0
//! cache_miss_per_m_usd = 0.20
//! cache_creation_per_m_usd = 0.0
//! output_per_m_usd = 0.40
//! ```
//!
//! The override merges on top of the hardcoded provider table:
//! [`super::compute_cost`] consults [`load_override_for`] first and falls
//! back to [`super::pricing_for`] when no override matches.
//!
//! ## Why this exists
//!
//! Adapted from `DeepSeek-Reasonix`'s `pricingOverride` config layer.
//! Lets users route through OpenAI-compatible gateways (OpenRouter,
//! DashScope, Azure deployments) that quote different prices for the
//! same DeepSeek models without forking the source tree.
//!
//! ## P3a scope
//!
//! Override **values** are honored end-to-end. Provider-level metadata
//! (api_key / base_url / proxy) remains a P5+ concern; this module
//! reads only the `pricing` array.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::RwLock;
use serde::Deserialize;

use super::ModelPricing;

/// One override entry — partial, but every field is required to take
/// effect. A pricing record is only injected when **all four** numeric
/// fields are present and non-negative; otherwise the override is
/// ignored (with a warn log) and the hardcoded record is used as-is.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct PricingOverrideEntry {
    pub model: String,
    pub cache_read_per_m_usd: f64,
    pub cache_miss_per_m_usd: f64,
    #[serde(default)]
    pub cache_creation_per_m_usd: f64,
    pub output_per_m_usd: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProviderOverrideSection {
    #[serde(default)]
    pricing: Vec<PricingOverrideEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OverrideRoot {
    #[serde(default)]
    providers: HashMap<String, ProviderOverrideSection>,
}

/// Process-wide override cache. Populated lazily on first access; flushed
/// only by [`reload_overrides_for_test`] (test-only).
static CACHE: OnceLock<RwLock<HashMap<String, HashMap<String, ModelPricing>>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<String, HashMap<String, ModelPricing>>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Default config path: `<dirs::config_dir()>/crown/config.toml`.
/// `None` only on platforms where `dirs` can't resolve a config dir
/// (rare; effectively never on Windows/macOS/Linux).
fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("crown").join("config.toml"))
}

/// Look up an override pricing record for `(provider, model)`. Returns
/// `None` if no override exists or the file is missing / invalid — the
/// caller is expected to fall back to the hardcoded provider table.
///
/// On first call, parses the config file once and caches by
/// `(provider, model)`. Subsequent calls are cheap reads.
pub fn load_override_for(provider: &str, model: &str) -> Option<ModelPricing> {
    {
        let guard = cache().read();
        if let Some(provider_table) = guard.get(provider) {
            if let Some(p) = provider_table.get(model) {
                return Some(*p);
            }
            // Already loaded this provider but no override for this model.
            // Don't reload from disk — file unchanged since last call.
            if !guard.is_empty() {
                return None;
            }
        }
    }
    // Cache miss: load from disk (lazy + idempotent).
    populate_cache_from_disk(default_config_path());
    let guard = cache().read();
    guard.get(provider).and_then(|t| t.get(model)).copied()
}

/// Test-only: clear the in-memory cache and reload from a custom path.
/// Public for use in `tests/`; not exposed via `pub use`.
#[cfg(test)]
pub(super) fn reload_overrides_for_test(path: Option<PathBuf>) {
    cache().write().clear();
    populate_cache_from_disk(path);
}

fn populate_cache_from_disk(path: Option<PathBuf>) {
    let Some(path) = path else { return };
    if !path.exists() {
        return;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "pricing override read failed");
            return;
        }
    };
    let root: OverrideRoot = match toml::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "pricing override parse failed");
            return;
        }
    };

    let mut new_table: HashMap<String, HashMap<String, ModelPricing>> = HashMap::new();
    for (provider, section) in root.providers {
        let mut by_model = HashMap::new();
        for entry in section.pricing {
            if let Some(p) = entry_to_pricing(&entry) {
                by_model.insert(entry.model.clone(), p);
            } else {
                tracing::warn!(
                    provider = %provider,
                    model = %entry.model,
                    "pricing override entry skipped: invalid or negative numbers",
                );
            }
        }
        new_table.insert(provider, by_model);
    }

    let mut guard = cache().write();
    *guard = new_table;
}

fn entry_to_pricing(e: &PricingOverrideEntry) -> Option<ModelPricing> {
    if e.cache_read_per_m_usd < 0.0
        || e.cache_miss_per_m_usd < 0.0
        || e.cache_creation_per_m_usd < 0.0
        || e.output_per_m_usd < 0.0
    {
        return None;
    }
    Some(ModelPricing {
        cache_read_per_m_usd: e.cache_read_per_m_usd,
        cache_miss_per_m_usd: e.cache_miss_per_m_usd,
        cache_creation_per_m_usd: e.cache_creation_per_m_usd,
        output_per_m_usd: e.output_per_m_usd,
        // Override entries don't carry these fields; substitute neutral
        // values. Any caller that relies on context_window / label /
        // description should use the hardcoded record (which provides
        // them) — the override is only for cost arithmetic.
        context_window: 0,
        label: "Custom (override)",
        description: "User-defined pricing override",
    })
}

/// Test entrypoint: write the given TOML to a temp file and reload the
/// cache from there. Returns the temp dir to keep the file alive for the
/// duration of the test.
#[cfg(test)]
pub(super) fn install_override_for_test(toml_str: &str) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, toml_str).expect("write toml");
    reload_overrides_for_test(Some(path));
    tmp
}

#[cfg(test)]
mod tests {
    //! Override cache is process-global. These tests mutate that shared cache,
    //! so they MUST run serially — a module-level `SERIAL` mutex guards every
    //! test (acquired at the top, held for the whole test). Without it,
    //! `install_override_for_test` (which replaces the entire cache) races
    //! across parallel tests and a just-installed override gets clobbered by a
    //! neighbour before the asserting test reads it.
    #![allow(clippy::unwrap_used)]

    use std::sync::Mutex;

    use super::*;

    /// Serializes all tests that touch the process-global override cache.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Lock the serial guard, recovering from a poisoned mutex (a panicking
    /// test shouldn't wedge the rest — the data is just `()`).
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn reset() {
        reload_overrides_for_test(None);
    }

    #[test]
    fn missing_file_yields_no_override() {
        let _s = serial();
        reset();
        assert!(load_override_for("anyone", "anymodel").is_none());
    }

    #[test]
    fn full_override_replaces_pricing() {
        let _s = serial();
        let _g = install_override_for_test(
            r#"
            [[providers.openrouter.pricing]]
            model = "deepseek/deepseek-v4-flash"
            cache_read_per_m_usd = 0.0
            cache_miss_per_m_usd = 0.20
            cache_creation_per_m_usd = 0.0
            output_per_m_usd = 0.40
            "#,
        );

        let p =
            load_override_for("openrouter", "deepseek/deepseek-v4-flash").expect("override hit");
        assert_eq!(p.cache_miss_per_m_usd, 0.20);
        assert_eq!(p.output_per_m_usd, 0.40);
        assert_eq!(p.label, "Custom (override)");

        // Different model in same provider -> no override
        assert!(load_override_for("openrouter", "other-model").is_none());
        // Different provider -> no override
        assert!(load_override_for("missing-provider", "any").is_none());

        reset();
    }

    /// Regression (P0-2): a user pricing override must NOT zero out the
    /// model's context window. Overrides carry `context_window: 0` (a
    /// placeholder — they only describe cost), so `context_window()` must
    /// ignore the override and use the hardcoded table. Returning 0 here
    /// makes the compaction ratio `prompt_tokens / 0` = inf/NaN, so folding
    /// never triggers and the next request blows past the window → 400.
    #[test]
    fn override_does_not_zero_context_window() {
        use crate::pricing::{context_window, ProviderId};

        let _s = serial();
        // Install an override for a model that HAS a hardcoded context window.
        let _g = install_override_for_test(
            r#"
            [[providers.deepseek.pricing]]
            model = "deepseek-v4-flash"
            cache_read_per_m_usd = 0.001
            cache_miss_per_m_usd = 0.10
            cache_creation_per_m_usd = 0.0
            output_per_m_usd = 0.20
            "#,
        );

        // The override is active for cost...
        assert!(
            load_override_for("deepseek", "deepseek-v4-flash").is_some(),
            "override should be installed"
        );

        // ...but the context window must still be the real (non-zero) value,
        // never the override's placeholder 0.
        let win = context_window(ProviderId::Deepseek, "deepseek-v4-flash");
        assert!(
            win > 0,
            "context_window must not be zeroed by a pricing override (got {win})"
        );

        reset();
    }

    #[test]
    fn negative_numbers_skip_entry_with_warn() {
        let _s = serial();
        let _g = install_override_for_test(
            r#"
            [[providers.bad.pricing]]
            model = "negatron"
            cache_read_per_m_usd = -1.0
            cache_miss_per_m_usd = 0.20
            output_per_m_usd = 0.40
            "#,
        );
        assert!(load_override_for("bad", "negatron").is_none());
        reset();
    }

    #[test]
    fn malformed_toml_does_not_crash_callers() {
        let _s = serial();
        // Write garbage TOML; cache stays empty, lookups return None.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml at = all").unwrap();
        reload_overrides_for_test(Some(path));

        assert!(load_override_for("anyone", "anymodel").is_none());
        reset();
    }
}
