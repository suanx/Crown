//! DeepSeek model pricing.
//!
//! ## Pricing source
//!
//! <https://api-docs.deepseek.com/quick_start/pricing>
//!
//! Last synced: 2026-05-28
//!
//! Numbers below are the **long-term standard pricing** that DeepSeek
//! announced has converted from a previous promotional schedule (per
//! the user's check against the official notice on 2026-05-28). v4-pro
//! is therefore priced at the same $0.003625 / $0.435 / $0.87 numbers
//! that DeepSeek-Reasonix runs in production.
//!
//! Cache creation is always 0 — DeepSeek does not bill a separate cache
//! creation tier (unlike Anthropic).
//!
//! ## When DeepSeek changes the price sheet
//!
//! 1. Update the constants below.
//! 2. Update `crates/core/tests/pricing_deepseek_test.rs::matches_published_sheet`.
//! 3. Bump "Last synced" date in this header.
//! 4. Snapshot tests will break before shipping if step 2 is forgotten.

use super::ModelPricing;

pub const FLASH: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.0028,
    cache_miss_per_m_usd: 0.14,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 0.28,
    context_window: 1_000_000,
    label: "DeepSeek V4 Flash",
    description: "Fast and cost-effective. Best for most tasks.",
};

pub const PRO: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.003625,
    cache_miss_per_m_usd: 0.435,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 0.87,
    context_window: 1_000_000,
    label: "DeepSeek V4 Pro",
    description: "Highest quality reasoning, coding, agentic.",
};

/// Compatibility aliases — `deepseek-chat` and `deepseek-reasoner` map
/// to flash's non-thinking and thinking modes respectively per the
/// deprecation notice. Prices identical to flash so old configs keep
/// working without a forced migration.
pub fn pricing_for(model: &str) -> Option<&'static ModelPricing> {
    match model {
        "deepseek-v4-flash" | "deepseek-chat" | "deepseek-reasoner" => Some(&FLASH),
        "deepseek-v4-pro" => Some(&PRO),
        _ => None,
    }
}

/// All known model ids paired with their pricing record. Order = UI
/// display order; `list_models` enumerates this directly so a single
/// edit here covers both pricing and the UI catalog.
pub fn all_models() -> &'static [(&'static str, &'static ModelPricing)] {
    &[("deepseek-v4-flash", &FLASH), ("deepseek-v4-pro", &PRO)]
}
