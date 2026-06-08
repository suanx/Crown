//! BPE encoder for DeepSeek V4 tokenizer.
//!
//! Mirrors Reasonix `src/tokenizer.ts`: byte-level BPE with GPT-2
//! byte→unicode mapping, 3 Split pre-tokenizer regexes, and added tokens.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;

use flate2::read::GzDecoder;
use lru::LruCache;
use regex::Regex;
use serde::Deserialize;

// ── Tokenizer JSON schema ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenizerData {
    added_tokens: Vec<AddedToken>,
    pre_tokenizer: PreTokenizerSeq,
    model: BpeModel,
}

#[derive(Deserialize)]
struct AddedToken {
    id: u32,
    content: String,
    special: bool,
    #[allow(dead_code)]
    normalized: bool,
}

#[derive(Deserialize)]
struct PreTokenizerSeq {
    #[allow(dead_code)]
    r#type: String,
    pretokenizers: Vec<Pretokenizer>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Pretokenizer {
    Split {
        pattern: PatternDef,
        #[allow(dead_code)]
        behavior: String,
        #[allow(dead_code)]
        invert: bool,
    },
    ByteLevel {
        #[allow(dead_code)]
        add_prefix_space: bool,
        #[allow(dead_code)]
        trim_offsets: bool,
        #[allow(dead_code)]
        use_regex: bool,
    },
}

#[derive(Deserialize)]
struct PatternDef {
    #[serde(rename = "Regex")]
    regex: String,
}

#[derive(Deserialize)]
struct BpeModel {
    #[allow(dead_code)]
    r#type: String,
    vocab: HashMap<String, u32>,
    merges: Vec<String>,
}

// ── Loaded tokenizer ───────────────────────────────────────────────────────

/// Fully initialized tokenizer ready for encoding.
pub struct LoadedTokenizer {
    vocab: HashMap<String, u32>,
    #[allow(dead_code)]
    merge_rank: Vec<(String, String)>, // indexed by rank
    merge_lookup: HashMap<String, usize>, // "a b" → rank
    split_regexes: Vec<Regex>,
    byte_to_char: [char; 256],
    /// Non-special added tokens sorted longest-first for greedy matching.
    added_pattern: Option<Regex>,
    added_map: HashMap<String, u32>,
    /// LRU cache for BPE encoding of repeated pieces.
    bpe_cache: Mutex<LruCache<String, Vec<String>>>,
}

/// Decompress and parse the tokenizer from gzipped JSON bytes.
pub fn load_from_gz(gz_data: &[u8]) -> LoadedTokenizer {
    let mut decoder = GzDecoder::new(gz_data);
    let mut json_str = String::new();
    decoder
        .read_to_string(&mut json_str)
        .expect("failed to decompress tokenizer data");

    let data: TokenizerData =
        serde_json::from_str(&json_str).expect("failed to parse tokenizer JSON");

    // Build merge rank lookup.
    let mut merge_lookup = HashMap::with_capacity(data.model.merges.len());
    let mut merge_rank = Vec::with_capacity(data.model.merges.len());
    for (i, m) in data.model.merges.iter().enumerate() {
        merge_lookup.insert(m.clone(), i);
        let parts: Vec<&str> = m.splitn(2, ' ').collect();
        if parts.len() == 2 {
            merge_rank.push((parts[0].to_string(), parts[1].to_string()));
        } else {
            merge_rank.push((m.clone(), String::new()));
        }
    }

    // Build split regexes from the Sequence pre-tokenizer.
    let mut split_regexes = Vec::new();
    for p in &data.pre_tokenizer.pretokenizers {
        if let Pretokenizer::Split { pattern, .. } = p {
            // The HuggingFace regex patterns use Unicode-aware matching.
            match Regex::new(&pattern.regex) {
                Ok(re) => split_regexes.push(re),
                Err(_) => {
                    // Skip unparseable regex — shouldn't happen with the
                    // shipped tokenizer data, but don't panic if it does.
                }
            }
        }
    }

    // Build added tokens (non-special only).
    let mut added_map = HashMap::new();
    let mut added_contents: Vec<String> = Vec::new();
    for t in &data.added_tokens {
        if !t.special {
            added_map.insert(t.content.clone(), t.id);
            added_contents.push(t.content.clone());
        }
    }
    // Longest-first for greedy matching.
    added_contents.sort_by_key(|b| std::cmp::Reverse(b.len()));
    let added_pattern = if added_contents.is_empty() {
        None
    } else {
        let escaped: Vec<String> = added_contents.iter().map(|s| regex::escape(s)).collect();
        // Inputs are `regex::escape`d so this should always compile, but match
        // the split_regexes path's "skip on error, never panic" policy: a
        // failure here just disables added-token splitting (BPE still works).
        Regex::new(&escaped.join("|")).ok()
    };

    LoadedTokenizer {
        vocab: data.model.vocab,
        merge_rank,
        merge_lookup,
        split_regexes,
        byte_to_char: build_byte_to_char(),
        added_pattern,
        added_map,
        bpe_cache: Mutex::new(LruCache::new(std::num::NonZeroUsize::new(8192).unwrap())),
    }
}

impl LoadedTokenizer {
    /// Encode text into token IDs.
    pub fn encode(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::new();

        let process_segment = |segment: &str, ids: &mut Vec<u32>| {
            if segment.is_empty() {
                return;
            }
            // Apply split pre-tokenizers.
            let mut chunks = vec![segment.to_string()];
            for re in &self.split_regexes {
                chunks = apply_split(&chunks, re);
            }
            // BPE encode each chunk.
            for chunk in &chunks {
                if chunk.is_empty() {
                    continue;
                }
                let byte_level = self.byte_level_encode(chunk);
                let pieces = self.bpe_encode(&byte_level);
                for p in &pieces {
                    if let Some(&id) = self.vocab.get(p) {
                        ids.push(id);
                    }
                    // If not in vocab, silently skip (shouldn't happen for
                    // byte-level BPE but we prefer under-count over panic).
                }
            }
        };

        if let Some(ref added_re) = self.added_pattern {
            let mut last = 0;
            for m in added_re.find_iter(text) {
                if m.start() > last {
                    process_segment(&text[last..m.start()], &mut ids);
                }
                if let Some(&id) = self.added_map.get(m.as_str()) {
                    ids.push(id);
                }
                last = m.end();
            }
            if last < text.len() {
                process_segment(&text[last..], &mut ids);
            }
        } else {
            process_segment(text, &mut ids);
        }

        ids
    }

    /// GPT-2 byte-level encoding: UTF-8 bytes → visible unicode chars.
    fn byte_level_encode(&self, s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(bytes.len());
        for &b in bytes {
            out.push(self.byte_to_char[b as usize]);
        }
        out
    }

    /// BPE encode a single byte-level piece, with LRU caching.
    fn bpe_encode(&self, piece: &str) -> Vec<String> {
        if piece.len() <= 1 {
            return if piece.is_empty() {
                Vec::new()
            } else {
                vec![piece.to_string()]
            };
        }

        // Check cache.
        {
            let mut cache = self.bpe_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.get(piece) {
                return cached.clone();
            }
        }

        // BPE merge loop.
        let mut word: Vec<String> = piece.chars().map(|c| c.to_string()).collect();
        while word.len() > 1 {
            let mut best_rank = usize::MAX;
            let mut best_idx = usize::MAX;
            for i in 0..word.len() - 1 {
                let pair = format!("{} {}", word[i], word[i + 1]);
                if let Some(&rank) = self.merge_lookup.get(&pair) {
                    if rank < best_rank {
                        best_rank = rank;
                        best_idx = i;
                        if rank == 0 {
                            break;
                        }
                    }
                }
            }
            if best_idx == usize::MAX {
                break;
            }
            let merged = format!("{}{}", word[best_idx], word[best_idx + 1]);
            word.splice(best_idx..=best_idx + 1, std::iter::once(merged));
        }

        // Store in cache.
        {
            let mut cache = self.bpe_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.put(piece.to_string(), word.clone());
        }

        word
    }
}

/// Apply a split regex: matches become their own chunks, in-between text also
/// becomes chunks (Isolated behavior).
fn apply_split(chunks: &[String], re: &Regex) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }
        let mut last = 0;
        for m in re.find_iter(chunk) {
            if m.start() > last {
                out.push(chunk[last..m.start()].to_string());
            }
            if !m.as_str().is_empty() {
                out.push(m.as_str().to_string());
            }
            last = m.end();
        }
        if last < chunk.len() {
            out.push(chunk[last..].to_string());
        }
    }
    out
}

/// Build GPT-2 byte→unicode char mapping (identical to Reasonix's
/// `buildByteToChar()`).
fn build_byte_to_char() -> [char; 256] {
    let mut result = ['\0'; 256];
    let mut bs: Vec<u8> = Vec::new();
    let mut cs: Vec<u32> = Vec::new();

    // Printable ASCII + Latin-1 supplement ranges.
    for b in 33u8..=126 {
        bs.push(b);
    }
    for b in 161u8..=172 {
        bs.push(b);
    }
    for b in 174u8..=255 {
        bs.push(b);
    }
    cs.extend(bs.iter().map(|&b| b as u32));

    let mut n: u32 = 0;
    for b in 0u8..=255 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(256 + n);
            n += 1;
        }
    }

    for (i, &b) in bs.iter().enumerate() {
        result[b as usize] = char::from_u32(cs[i]).unwrap_or('\u{FFFD}');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_char_covers_all_256() {
        let table = build_byte_to_char();
        for b in 0..256u16 {
            assert_ne!(table[b as usize], '\0', "byte {b} unmapped");
        }
    }

    #[test]
    fn load_from_gz_succeeds() {
        let gz = include_bytes!("../data/deepseek-tokenizer.json.gz");
        let tok = load_from_gz(gz);
        assert!(!tok.vocab.is_empty());
        assert!(!tok.merge_rank.is_empty());
        assert!(!tok.split_regexes.is_empty());
    }

    #[test]
    fn encode_basic_ascii() {
        let gz = include_bytes!("../data/deepseek-tokenizer.json.gz");
        let tok = load_from_gz(gz);
        let ids = tok.encode("hello");
        assert!(!ids.is_empty(), "should produce at least one token");
    }
}
