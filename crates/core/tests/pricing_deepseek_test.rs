//! DeepSeek pricing snapshot — break me when DeepSeek changes the
//! price sheet at <https://api-docs.deepseek.com/quick_start/pricing>.

use deepseek_core::pricing::{
    self,
    deepseek::{FLASH, PRO},
    ProviderId, UsageBreakdown,
};

#[test]
fn matches_published_sheet_2026_05_28() {
    // Flash — cheap tier, identical for the chat/reasoner aliases.
    assert_eq!(FLASH.cache_read_per_m_usd, 0.0028);
    assert_eq!(FLASH.cache_miss_per_m_usd, 0.14);
    assert_eq!(FLASH.cache_creation_per_m_usd, 0.0);
    assert_eq!(FLASH.output_per_m_usd, 0.28);
    assert_eq!(FLASH.context_window, 1_000_000);

    // Pro — long-term standard pricing per 2026-05-28 notice.
    assert_eq!(PRO.cache_read_per_m_usd, 0.003625);
    assert_eq!(PRO.cache_miss_per_m_usd, 0.435);
    assert_eq!(PRO.cache_creation_per_m_usd, 0.0);
    assert_eq!(PRO.output_per_m_usd, 0.87);
    assert_eq!(PRO.context_window, 1_000_000);
}

#[test]
fn pro_strictly_above_flash_each_tier() {
    // These are const comparisons — clippy flags them as obvious truths,
    // but the *point* of this test is to act as a snapshot guard against
    // someone editing the price tables in a way that violates the
    // model-tier invariant. Allow the lint; the test is for humans.
    #![allow(clippy::assertions_on_constants)]
    assert!(PRO.cache_read_per_m_usd > FLASH.cache_read_per_m_usd);
    assert!(PRO.cache_miss_per_m_usd > FLASH.cache_miss_per_m_usd);
    assert!(PRO.output_per_m_usd > FLASH.output_per_m_usd);
}

#[test]
fn flash_one_million_each_tier() {
    let u = UsageBreakdown {
        cache_read_tokens: 1_000_000,
        cache_miss_tokens: 1_000_000,
        cache_creation_tokens: 0,
        output_tokens: 1_000_000,
    };
    let c = pricing::compute_cost(ProviderId::Deepseek, "deepseek-v4-flash", u);
    let expected = FLASH.cache_read_per_m_usd + FLASH.cache_miss_per_m_usd + FLASH.output_per_m_usd;
    assert!(
        (c - expected).abs() < 1e-9,
        "cost = {c}, expected {expected}"
    );
}

#[test]
fn aliases_priced_as_flash() {
    let u = UsageBreakdown {
        output_tokens: 1000,
        ..Default::default()
    };
    let chat = pricing::compute_cost(ProviderId::Deepseek, "deepseek-chat", u);
    let reasoner = pricing::compute_cost(ProviderId::Deepseek, "deepseek-reasoner", u);
    let flash = pricing::compute_cost(ProviderId::Deepseek, "deepseek-v4-flash", u);
    assert_eq!(chat, flash);
    assert_eq!(reasoner, flash);
}

#[test]
fn cache_savings_typical_request() {
    // 1M cache hits: savings = (0.14 - 0.0028) × 1.0 = $0.1372
    let s = pricing::cache_savings_usd(ProviderId::Deepseek, "deepseek-v4-flash", 1_000_000);
    assert!((s - 0.1372).abs() < 1e-9, "savings = {s}");
}

#[test]
fn unknown_model_returns_zero_cost() {
    let u = UsageBreakdown {
        output_tokens: 1000,
        ..Default::default()
    };
    assert_eq!(
        pricing::compute_cost(ProviderId::Deepseek, "ghost-model-9000", u),
        0.0
    );
}

#[test]
fn pro_typical_request_arithmetic() {
    // 10K uncached prompt + 2K output, no cache hit
    let u = UsageBreakdown {
        cache_read_tokens: 0,
        cache_miss_tokens: 10_000,
        cache_creation_tokens: 0,
        output_tokens: 2_000,
    };
    let c = pricing::compute_cost(ProviderId::Deepseek, "deepseek-v4-pro", u);
    // 10K * 0.435 / 1M + 2K * 0.87 / 1M = 0.00435 + 0.00174 = 0.00609
    let expected = 10_000.0 * 0.435 / 1_000_000.0 + 2_000.0 * 0.87 / 1_000_000.0;
    assert!(
        (c - expected).abs() < 1e-9,
        "c = {c}, expected = {expected}"
    );
}

#[test]
fn all_models_returns_two_entries_in_order() {
    let all = pricing::deepseek::all_models();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].0, "deepseek-v4-flash");
    assert_eq!(all[1].0, "deepseek-v4-pro");
}
