//! DeepSeek V4 BPE tokenizer — encode-only port for token counting and
//! context window estimation.
//!
//! Embeds the compressed vocab/merges data (~1.7MB gz) at compile time.
//! First call to any public function decompresses and initializes the
//! singleton (~100ms, ~35MB heap). Call [`warmup()`] at app idle to avoid
//! paying this cost on the first user turn.
//!
//! # Design (mirrors Reasonix `src/tokenizer.ts`)
//!
//! - Byte-level BPE with GPT-2 byte→unicode mapping
//! - 3 Split pre-tokenizer regexes (Sequence type)
//! - Added tokens (non-special) handled via longest-first greedy
//! - V4 chat template for accurate `prompt_tokens` estimation
//! - LRU caches for repeated BPE segments and content token counts

mod bpe;
mod types;
pub mod template;

use std::sync::OnceLock;

use bpe::LoadedTokenizer;

pub use template::{
    estimate_conversation_tokens, estimate_request_tokens, format_deepseek_prompt, ChatMessage,
    ToolCallEntry, ToolCallFunction,
};
pub use types::MessageContent;


/// Compressed tokenizer data (vocab + merges + pre_tokenizer config).
/// `include_bytes!` embeds at compile time — no runtime file I/O.
static TOKENIZER_GZ: &[u8] = include_bytes!("../data/deepseek-tokenizer.json.gz");

/// Global singleton. Initialized on first use via [`get_tokenizer()`].
static TOKENIZER: OnceLock<LoadedTokenizer> = OnceLock::new();

/// Access the initialized tokenizer. Decompresses + parses on first call.
fn get_tokenizer() -> &'static LoadedTokenizer {
    TOKENIZER.get_or_init(|| bpe::load_from_gz(TOKENIZER_GZ))
}

/// Pre-warm the tokenizer singleton at app idle. Idempotent.
///
/// Call once after first paint / after app setup so the first user turn
/// doesn't pay the ~100ms init cost.
pub fn warmup() {
    let _ = get_tokenizer();
}

/// Encode text into token IDs using DeepSeek V4 BPE.
pub fn encode(text: &str) -> Vec<u32> {
    if text.is_empty() {
        return Vec::new();
    }
    get_tokenizer().encode(text)
}

/// Count tokens in `text` (exact).
pub fn count_tokens(text: &str) -> usize {
    encode(text).len()
}

/// Default character limit for the bounded estimator's exact-encode window.
pub const DEFAULT_BOUNDED_TOKENIZE_CHARS: usize = 2048;

/// Count tokens with a bounded cost: texts ≤ `max_chars` are encoded exactly;
/// longer texts sample head + tail and extrapolate.
///
/// Accuracy: ±5-10% for long texts, exact for short texts.
pub fn count_tokens_bounded(text: &str, max_chars: usize) -> usize {
    if text.is_empty() {
        return 0;
    }
    let cap = max_chars;
    if text.len() <= cap {
        return count_tokens(text);
    }
    if cap == 0 {
        // Fallback ratio when no sampling is possible.
        return (text.len() as f64 * 0.3).ceil().max(1.0) as usize;
    }

    let head_chars = cap.div_ceil(2);
    let tail_chars = cap / 2;
    let head = &text[..safe_char_boundary(text, head_chars)];
    let tail = if tail_chars > 0 {
        &text[safe_char_boundary_rev(text, tail_chars)..]
    } else {
        ""
    };
    let sample_chars = head.len() + tail.len();
    let sample_tokens = count_tokens(head) + count_tokens(tail);
    let ratio = if sample_chars > 0 {
        sample_tokens as f64 / sample_chars as f64
    } else {
        0.3
    };
    (text.len() as f64 * ratio).ceil().max(1.0) as usize
}

/// Find the nearest char boundary at or before `byte_offset`.
fn safe_char_boundary(s: &str, byte_offset: usize) -> usize {
    let offset = byte_offset.min(s.len());
    let mut i = offset;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the nearest char boundary for the last `tail_bytes` of `s`.
fn safe_char_boundary_rev(s: &str, tail_bytes: usize) -> usize {
    if tail_bytes >= s.len() {
        return 0;
    }
    let target = s.len() - tail_bytes;
    let mut i = target;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_is_idempotent() {
        warmup();
        warmup();
        // Just ensure no panic.
    }

    #[test]
    fn encode_empty_returns_empty() {
        assert!(encode("").is_empty());
    }

    #[test]
    fn count_tokens_hello_world() {
        let n = count_tokens("Hello, world!");
        // DeepSeek V4 BPE should produce a small number of tokens for this.
        assert!(n > 0 && n < 10, "got {n}");
    }

    #[test]
    fn count_tokens_bounded_short_is_exact() {
        let text = "short text";
        assert_eq!(
            count_tokens_bounded(text, DEFAULT_BOUNDED_TOKENIZE_CHARS),
            count_tokens(text),
        );
    }

    #[test]
    fn count_tokens_bounded_long_is_approximate() {
        let text = "a ".repeat(5000); // ~10000 chars
        let exact = count_tokens(&text);
        let bounded = count_tokens_bounded(&text, DEFAULT_BOUNDED_TOKENIZE_CHARS);
        // Should be within 15% of exact for this uniform text.
        let ratio = bounded as f64 / exact as f64;
        assert!(
            (0.85..=1.15).contains(&ratio),
            "exact={exact}, bounded={bounded}, ratio={ratio:.3}",
        );
    }
}
