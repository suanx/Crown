//! Anthropic model pricing.
//!
//! Source: <https://www.anthropic.com/pricing> (last synced 2026-06-12)
//!
//! Anthropic bills `cache_creation_input_tokens` separately at 1.25× the
//! normal input rate. `cache_read_input_tokens` are billed at 0.1× normal.

use super::ModelPricing;

const SONNET: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.30,
    cache_miss_per_m_usd: 3.00,
    cache_creation_per_m_usd: 3.75,
    output_per_m_usd: 15.00,
    context_window: 200_000,
    label: "Claude Sonnet 4",
    description: "High-intelligence flagship.",
};

const HAIKU: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.08,
    cache_miss_per_m_usd: 0.80,
    cache_creation_per_m_usd: 1.00,
    output_per_m_usd: 4.00,
    context_window: 200_000,
    label: "Claude Haiku 3.5",
    description: "Fast, affordable, lightweight.",
};

const OPUS: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 1.50,
    cache_miss_per_m_usd: 15.00,
    cache_creation_per_m_usd: 18.75,
    output_per_m_usd: 75.00,
    context_window: 200_000,
    label: "Claude Opus 4",
    description: "Peak intelligence and reasoning.",
};

pub fn pricing_for(model: &str) -> Option<&'static ModelPricing> {
    match model {
        m if m.starts_with("claude-sonnet-4") => Some(&SONNET),
        m if m.starts_with("claude-haiku-3") => Some(&HAIKU),
        m if m.starts_with("claude-opus-4") => Some(&OPUS),
        m if m.starts_with("claude-3-5-sonnet") => Some(&SONNET),
        m if m.starts_with("claude-3-haiku") => Some(&HAIKU),
        _ => None,
    }
}

pub fn all_models() -> &'static [(&'static str, &'static ModelPricing)] {
    &[
        ("claude-sonnet-4-20250514", &SONNET),
        ("claude-3-5-haiku-20241022", &HAIKU),
        ("claude-opus-4-20250514", &OPUS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_pricing() {
        let p = pricing_for("claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.context_window, 200_000);
        assert!(p.cache_miss_per_m_usd > 0.0);
    }

    #[test]
    fn haiku_pricing() {
        let p = pricing_for("claude-3-5-haiku-20241022").unwrap();
        assert!(p.cache_miss_per_m_usd < p.output_per_m_usd);
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(pricing_for("claude-instant-1.2").is_none());
    }
}
