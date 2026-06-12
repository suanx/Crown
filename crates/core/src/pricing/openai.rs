//! OpenAI model pricing.
//!
//! Source: <https://openai.com/api/pricing/> (last synced 2026-06-12)
//!
//! OpenAI bills a single `cached_tokens` tier (no separate creation tier
//! like Anthropic), so `cache_creation_per_m_usd` is 0.0.

use super::ModelPricing;

const GPT_5_5: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 5.00,
    cache_miss_per_m_usd: 10.00,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 40.00,
    context_window: 128_000,
    label: "GPT-5.5",
    description: "Highest intelligence reasoning model.",
};

const GPT_5_MINI: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.075,
    cache_miss_per_m_usd: 0.15,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 0.60,
    context_window: 128_000,
    label: "GPT-5-mini",
    description: "Fast, cost-efficient for light tasks.",
};

const O4_MINI: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 0.55,
    cache_miss_per_m_usd: 1.10,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 4.40,
    context_window: 200_000,
    label: "o4-mini",
    description: "Fast reasoning, small context.",
};

const O4: ModelPricing = ModelPricing {
    cache_read_per_m_usd: 5.00,
    cache_miss_per_m_usd: 10.00,
    cache_creation_per_m_usd: 0.0,
    output_per_m_usd: 40.00,
    context_window: 200_000,
    label: "o4",
    description: "Full reasoning model with tools.",
};

pub fn pricing_for(model: &str) -> Option<&'static ModelPricing> {
    match model {
        m if m.starts_with("gpt-5.5") || m.starts_with("gpt-5-5") => Some(&GPT_5_5),
        m if m.starts_with("gpt-5-mini") || m.starts_with("gpt-5.5-mini") => Some(&GPT_5_MINI),
        m if m.starts_with("o4-mini") => Some(&O4_MINI),
        m if m.starts_with("o4") && !m.starts_with("o4-mini") => Some(&O4),
        m if m.starts_with("o1") || m.starts_with("o3") => Some(&O4),
        m if m.starts_with("gpt-4o") => Some(&O4_MINI),
        _ => None,
    }
}

pub fn all_models() -> &'static [(&'static str, &'static ModelPricing)] {
    &[
        ("gpt-5.5", &GPT_5_5),
        ("gpt-5-mini", &GPT_5_MINI),
        ("o4-mini", &O4_MINI),
        ("o4", &O4),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt_5_5_pricing() {
        let p = pricing_for("gpt-5.5").unwrap();
        assert_eq!(p.context_window, 128_000);
        assert!(p.cache_miss_per_m_usd > 0.0);
    }

    #[test]
    fn gpt_5_mini_pricing() {
        let p = pricing_for("gpt-5-mini").unwrap();
        assert_eq!(p.context_window, 128_000);
    }

    #[test]
    fn o4_mini_pricing() {
        let p = pricing_for("o4-mini").unwrap();
        assert_eq!(p.context_window, 200_000);
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(pricing_for("ada").is_none());
    }
}
