//! Multi-provider pricing & cost computation.
//!
//! ## Architecture
//!
//! Per-provider pricing tables live in submodules ([`deepseek`],
//! [`openai`], [`anthropic`]). The top-level [`compute_cost`] dispatches
//! by [`ProviderId`].
//!
//! ## ⚠️ No price API for any of these
//!
//! As of 2026-05-28:
//! - DeepSeek: `/v1/models` only returns id + owner, no prices.
//! - OpenAI: `/v1/models` similarly bare; pricing only in docs.
//! - Anthropic: same.
//!
//! Tables are hardcoded; ALL changes touch the corresponding submodule
//! AND its snapshot test in `tests/`. Source URL + last-synced date are
//! mandatory in each submodule's header comment.
//!
//! ## Provider extensibility
//!
//! Adding a new provider:
//! 1. Add a variant to [`ProviderId`] (or rely on the [`ProviderId::Other`]
//!    fallthrough for fully-dynamic cases).
//! 2. Implement a submodule with `pricing_for(model)` that returns
//!    `Option<&'static ModelPricing>`.
//! 3. Wire it into the [`compute_cost`] / [`cache_savings_usd`] dispatch.
//! 4. Add a snapshot test under `crates/core/tests/pricing_<provider>_test.rs`.

use serde::{Deserialize, Serialize};

pub mod anthropic;
pub mod deepseek;
pub mod openai;
pub mod overrides;

/// Stable provider identifier. New variants append-only — never remove
/// or renumber, since DB rows reference these by string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    /// DeepSeek (the only provider implemented in P3a). Default for new
    /// threads + missing config so existing clients keep working.
    #[default]
    Deepseek,
    /// Anthropic (Claude models).
    #[serde(rename = "anthropic")]
    Anthropic,
    /// OpenAI (GPT / o-series models).
    #[serde(rename = "openai")]
    Openai,
    /// Forward-compatible catch-all for unknown / future providers.
    /// `compute_cost` returns 0.0 + warns; callers must treat as "no
    /// pricing available" rather than "free".
    #[serde(other)]
    Other,
}

impl ProviderId {
    /// Stable wire-format string. New variants must add a case here.
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Deepseek => "deepseek",
            ProviderId::Anthropic => "anthropic",
            ProviderId::Openai => "openai",
            ProviderId::Other => "other",
        }
    }

    /// Parse a provider string (case-insensitive) into our enum. Unknown
    /// strings map to [`ProviderId::Other`] so old DB rows / future
    /// configs degrade gracefully instead of erroring out.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "deepseek" => ProviderId::Deepseek,
            "anthropic" => ProviderId::Anthropic,
            "openai" => ProviderId::Openai,
            _ => ProviderId::Other,
        }
    }
}
}

/// Generic per-model pricing record. All fields USD per 1M tokens.
///
/// Field naming is **provider-agnostic** so the same struct works for
/// DeepSeek (cache hit/miss), OpenAI (cached input), Anthropic (cache
/// read + cache creation). Providers without a given tier write 0.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    /// USD/1M for **cache-read** input tokens (the cheap tier).
    /// DeepSeek: prompt_cache_hit. OpenAI: cached. Anthropic: cache_read.
    pub cache_read_per_m_usd: f64,
    /// USD/1M for **uncached** input tokens.
    /// DeepSeek: prompt_cache_miss. OpenAI: prompt - cached. Anthropic:
    /// input - cache_read - cache_creation.
    pub cache_miss_per_m_usd: f64,
    /// USD/1M for **cache-creation** input tokens (Anthropic-only;
    /// usually 1.25× normal input). DeepSeek/OpenAI = 0.
    pub cache_creation_per_m_usd: f64,
    /// USD/1M for output tokens.
    pub output_per_m_usd: f64,
    /// Context window in tokens (for UI display).
    pub context_window: u64,
    /// Human-readable label.
    pub label: &'static str,
    /// One-line description for UI.
    pub description: &'static str,
}

/// One turn's token counts. Fed by client `Usage` after provider-specific
/// extraction; all fields are post-normalization (cache miss already
/// excludes cache hit).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageBreakdown {
    pub cache_read_tokens: u64,
    pub cache_miss_tokens: u64,
    pub cache_creation_tokens: u64,
    pub output_tokens: u64,
}

impl UsageBreakdown {
    /// Map a provider's raw `Usage` into a normalized breakdown.
    ///
    /// ## Provider neutrality
    ///
    /// The cache-token field NAMES differ per provider, so the mapping is
    /// gated by `provider` (see `.kiro/steering/provider-neutrality.md`):
    /// - **DeepSeek**: `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`.
    /// - **Other** (OpenAI `cached_tokens`, Anthropic `cache_read_input_tokens`
    ///   / `cache_creation_input_tokens`): the current [`Usage`] struct does
    ///   not yet carry those fields, so we fall back to treating the whole
    ///   prompt as uncached input (`cache_miss = prompt_tokens`). This is the
    ///   safe, provider-correct default — it never mis-reports another
    ///   provider's input as "free cache hits". When those providers are wired
    ///   in, extend this match arm (and `Usage`) — do NOT read DeepSeek field
    ///   names unconditionally.
    pub fn from_usage(provider: ProviderId, u: &deepseek_client::types::Usage) -> Self {
        match provider {
            ProviderId::Deepseek => Self {
                cache_read_tokens: u.prompt_cache_hit_tokens as u64,
                cache_miss_tokens: u.prompt_cache_miss_tokens as u64,
                cache_creation_tokens: 0,
                output_tokens: u.completion_tokens as u64,
            },
            // Anthropic and OpenAI: fallback until Usage carries their
            // provider-specific cache fields.
            ProviderId::Anthropic | ProviderId::Openai | ProviderId::Other => Self {
                cache_read_tokens: 0,
                cache_miss_tokens: u.prompt_tokens as u64,
                cache_creation_tokens: 0,
                output_tokens: u.completion_tokens as u64,
            },
        }
    }
}

/// Resolve a `(provider, model)` pair to its [`ModelPricing`]. Returns
/// `None` for unknown providers or models — callers must treat that as
/// "cost unknown" (typically 0.0) rather than crashing.
///
/// User-level overrides win over the hardcoded provider table when they
/// match. Order:
///   1. `<config_dir>/crown/config.toml` `[providers.<id>.pricing]`
///   2. `super::deepseek::pricing_for(model)` (or other provider modules)
///   3. `None`
fn pricing_for(provider: ProviderId, model: &str) -> Option<&'static ModelPricing> {
    // Static fallback (the hardcoded tables). Override takes precedence
    // but it can't return a `&'static` because the override pricing is
    match provider {
        ProviderId::Deepseek => deepseek::pricing_for(model),
        ProviderId::Anthropic => anthropic::pricing_for(model),
        ProviderId::Openai => openai::pricing_for(model),
        ProviderId::Other => None,
    }
}

/// Like [`pricing_for`] but returns an owned [`ModelPricing`] so the
/// runtime-loaded override can win. Cheap because [`ModelPricing`] is
/// `Copy`.
fn pricing_for_owned(provider: ProviderId, model: &str) -> Option<ModelPricing> {
    if let Some(p) = overrides::load_override_for(provider.as_str(), model) {
        return Some(p);
    }
    pricing_for(provider, model).copied()
}

/// Returns the context window size in tokens for the given provider+model.
/// Falls back to 131_072 for unknown combinations.
///
/// Accepts an optional `custom_override` — when `Some(n > 0)`, that value is
/// returned instead of the hardcoded table entry, allowing users to set a
/// custom context window per model via the UI (Settings → 模型供应商).
///
/// **Still deliberately does NOT consult pricing overrides** (which carry a
/// placeholder `context_window: 0` — see pricing/overrides.rs).
pub fn context_window(provider: ProviderId, model: &str, custom_override: Option<usize>) -> usize {
    if let Some(custom) = custom_override {
        if custom > 0 {
            return custom;
        }
    }
    pricing_for(provider, model)
        .map(|p| p.context_window as usize)
        .unwrap_or(131_072)
}

/// Compute cost in USD for a given (provider, model, usage) tuple.
///
/// Returns 0.0 silently for unknown provider/model combinations. Caller
/// is responsible for any warn-logging if a non-zero result was expected.
pub fn compute_cost(provider: ProviderId, model: &str, u: UsageBreakdown) -> f64 {
    let Some(p) = pricing_for_owned(provider, model) else {
        return 0.0;
    };
    let m = 1_000_000.0;
    (u.cache_read_tokens as f64 / m) * p.cache_read_per_m_usd
        + (u.cache_miss_tokens as f64 / m) * p.cache_miss_per_m_usd
        + (u.cache_creation_tokens as f64 / m) * p.cache_creation_per_m_usd
        + (u.output_tokens as f64 / m) * p.output_per_m_usd
}

/// USD saved by cache hits vs cache miss for the given (provider, model).
/// Used for the "你省了多少 $" UI badge — DeepSeek's cache pricing is
/// ~50× cheaper than the miss tier so this number gets large quickly.
pub fn cache_savings_usd(provider: ProviderId, model: &str, cache_read_tokens: u64) -> f64 {
    if cache_read_tokens == 0 {
        return 0.0;
    }
    let Some(p) = pricing_for_owned(provider, model) else {
        return 0.0;
    };
    let savings_per_m = p.cache_miss_per_m_usd - p.cache_read_per_m_usd;
    if savings_per_m <= 0.0 {
        return 0.0;
    }
    (cache_read_tokens as f64 / 1_000_000.0) * savings_per_m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_returns_zero_cost() {
        let u = UsageBreakdown {
            output_tokens: 10_000,
            ..Default::default()
        };
        assert_eq!(compute_cost(ProviderId::Other, "anything", u), 0.0);
    }

    fn provider_id_round_trip_lossy() {
        assert_eq!(ProviderId::from_str_lossy("deepseek"), ProviderId::Deepseek);
        assert_eq!(ProviderId::from_str_lossy("DeepSeek"), ProviderId::Deepseek);
        assert_eq!(ProviderId::from_str_lossy("openai"), ProviderId::Openai);
        assert_eq!(ProviderId::from_str_lossy("anthropic"), ProviderId::Anthropic);
        assert_eq!(ProviderId::from_str_lossy(""), ProviderId::Other);
        assert_eq!(ProviderId::Deepseek.as_str(), "deepseek");
        assert_eq!(ProviderId::Anthropic.as_str(), "anthropic");
        assert_eq!(ProviderId::Openai.as_str(), "openai");
        assert_eq!(ProviderId::Other.as_str(), "other");
    }

    #[test]
    fn provider_id_default_is_deepseek() {
        assert_eq!(ProviderId::default(), ProviderId::Deepseek);
    }

    #[test]
    fn provider_id_serde_round_trip() {
        let s = serde_json::to_string(&ProviderId::Deepseek).unwrap();
        assert_eq!(s, "\"deepseek\"");
        let back: ProviderId = serde_json::from_str("\"deepseek\"").unwrap();
        assert_eq!(back, ProviderId::Deepseek);
        // Known strings parse to their variant
        let openai: ProviderId = serde_json::from_str("\"openai\"").unwrap();
        assert_eq!(openai, ProviderId::Openai);
        let anthropic: ProviderId = serde_json::from_str("\"anthropic\"").unwrap();
        assert_eq!(anthropic, ProviderId::Anthropic);
        // Unknown string parses to Other thanks to #[serde(other)]
        let other: ProviderId = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(other, ProviderId::Other);
    }

    #[test]
    fn cache_savings_zero_when_no_hits() {
        assert_eq!(
            cache_savings_usd(ProviderId::Deepseek, "deepseek-v4-flash", 0),
            0.0
        );
    }

    #[test]
    fn cache_savings_zero_for_unknown_provider() {
        assert_eq!(
            cache_savings_usd(ProviderId::Other, "any-model", 1_000_000),
            0.0
        );
    }

    /// P1-4: usage field mapping is provider-gated. DeepSeek reads its
    /// prompt_cache_hit/miss fields; other providers must NOT read those (the
    /// current Usage struct doesn't carry their cache fields) and instead
    /// treat the whole prompt as uncached input.
    #[test]
    fn usage_breakdown_maps_per_provider() {
        let u = deepseek_client::types::Usage {
            prompt_tokens: 1000,
            completion_tokens: 50,
            total_tokens: 1050,
            prompt_cache_hit_tokens: 800,
            prompt_cache_miss_tokens: 200,
        };

        // DeepSeek: uses its own cache hit/miss fields.
        let ds = UsageBreakdown::from_usage(ProviderId::Deepseek, &u);
        assert_eq!(ds.cache_read_tokens, 800);
        assert_eq!(ds.cache_miss_tokens, 200);
        assert_eq!(ds.output_tokens, 50);

        // Other: ignores DeepSeek-specific cache fields, treats all prompt
        // tokens as uncached input (never mis-reports as free cache hits).
        let other = UsageBreakdown::from_usage(ProviderId::Other, &u);
        assert_eq!(other.cache_read_tokens, 0);
        assert_eq!(other.cache_miss_tokens, 1000);
        assert_eq!(other.output_tokens, 50);
    }
}
