pub mod abort;
pub mod compaction;
pub mod context;
pub mod engine;
pub mod gate;
pub mod hooks;
pub mod memory;
pub mod paths;
pub mod permission;
pub mod pricing;
pub mod prompt;
pub mod repair;
pub mod rewind;
pub mod skills;
pub mod subagent;
pub mod thread;

/// Pre-warm the BPE tokenizer singleton (used by context compaction's size
/// estimates). Re-exported so the app crate can warm it at startup without
/// taking a direct dependency on the tokenizer crate. Idempotent; safe to
/// call from a background thread.
pub fn warmup_tokenizer() {
    deepseek_tokenizer::warmup();
}
