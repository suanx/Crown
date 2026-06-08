//! OpenAI model pricing — unimplemented placeholder for P5+.
//!
//! When implementing:
//! - GPT-5.5 / GPT-5-mini / o-series — see <https://openai.com/api/pricing/>
//! - OpenAI bills a single `cached_tokens` tier (no separate creation tier
//!   like Anthropic), so map to [`super::ModelPricing`] with
//!   `cache_creation_per_m_usd = 0.0`.

use super::ModelPricing;

pub fn pricing_for(_model: &str) -> Option<&'static ModelPricing> {
    None
}
