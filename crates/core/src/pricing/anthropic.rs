//! Anthropic model pricing — unimplemented placeholder for P5+.
//!
//! When implementing:
//! - Claude Opus 4.7 / Sonnet 4.6 / Haiku — see
//!   <https://www.anthropic.com/pricing#api>.
//! - Anthropic bills `cache_creation_input_tokens` separately at 1.25×
//!   the normal input rate. The [`super::ModelPricing`] struct already
//!   exposes `cache_creation_per_m_usd` for exactly this case.
//! - `cache_read_input_tokens` are billed at 0.1× normal input.

use super::ModelPricing;

pub fn pricing_for(_model: &str) -> Option<&'static ModelPricing> {
    None
}
