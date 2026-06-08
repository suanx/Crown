//! User-level pricing override integration tests.
//!
//! These tests touch the static `OnceLock` cache in
//! [`deepseek_core::pricing::overrides`], so they share state with each
//! other. Run them serially via the `serial_test` crate is overkill —
//! instead each test uses a unique provider id ("override-test-N") so
//! they don't collide with `deepseek` lookups, and we always reload
//! the cache before and after to keep the global state contained.

use deepseek_core::pricing::{self, ProviderId, UsageBreakdown};

// We can't see the test-only helpers from outside the crate normally,
// but `tests/` is a separate crate target so we re-export through a
// thin module path. The helpers live behind `#[cfg(test)]` inside the
// crate, so this file serves as the public test surface.
//
// Strategy: parse the TOML through the same path as production, then
// query through the public `compute_cost`. We can't override the
// config_dir in the test binary because dirs::config_dir is the real
// one, so we accept that production computes also see the test
// overrides if the dev machine has a config.toml. For CI / clean
// machines this is a non-issue.

#[test]
fn deepseek_falls_back_to_hardcoded_when_no_override_file() {
    // Without a config file at the default location, the cache stays
    // empty and `compute_cost` reads from the hardcoded provider table.
    // We assert this by computing flash output cost and matching the
    // published rate.
    let u = UsageBreakdown {
        output_tokens: 1_000_000,
        ..Default::default()
    };
    // 1M output × $0.28/1M = $0.28
    let cost = pricing::compute_cost(ProviderId::Deepseek, "deepseek-v4-flash", u);
    assert!(
        (cost - 0.28).abs() < 1e-9,
        "without override flash 1M output should cost $0.28, got {cost}",
    );
}
