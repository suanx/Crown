//! Cache-aware context compaction (auto-fold).
//!
//! Mirrors Reasonix `src/context-manager.ts`. Decides when to fold the
//! conversation log based on prompt_tokens / context_window ratio, then
//! summarizes the "head" (old) segment using a cheap model call while
//! preserving the prefix bytes for cache reuse.
//!
//! ## Design
//!
//! - Multi-threshold decision: 0.75 normal / 0.78 aggressive / 0.80 force-exit / 0.90 pre-fold
//! - Boundary always on a `user` role message (never splits tool_calls pair)
//! - Summary call reuses system prompt + tools verbatim (cache-aligned)
//! - Pinned constraints preserved across fold

use crate::pricing::{self, ProviderId};
use deepseek_client::types::Usage;

// ── Constants (match Reasonix) ─────────────────────────────────────────────

/// Normal fold threshold (ratio of prompt_tokens to context_window).
pub const FOLD_THRESHOLD: f64 = 0.75;
/// Tail budget as fraction of ctx_max during normal fold.
pub const FOLD_TAIL_FRACTION: f64 = 0.20;
/// Aggressive fold threshold.
pub const FOLD_AGGRESSIVE_THRESHOLD: f64 = 0.78;
/// Tail budget during aggressive fold.
pub const FOLD_AGGRESSIVE_TAIL_FRACTION: f64 = 0.10;
/// Above this, exit the turn with a summary instead of folding.
pub const FORCE_SUMMARY_THRESHOLD: f64 = 0.80;
/// Turn-start local estimate threshold for pre-iteration fold.
pub const TURN_START_FOLD_THRESHOLD: f64 = 0.90;
/// Skip fold if head wouldn't shrink log by at least this fraction.
pub const MIN_SAVINGS_FRACTION: f64 = 0.30;
/// Hard timeout for the summary model call (ms).
///
/// A fold summary can carry a near-full context window (hundreds of K
/// tokens), and slower / non-DeepSeek providers need real headroom to
/// respond — a too-tight bound silently turns every fold into a no-op (the
/// turn then risks 400ing on the over-budget context it failed to compact).
/// 45s is generous enough for large cross-provider summaries while still
/// bounding a genuinely hung request so it can't stall the turn loop.
pub const SUMMARY_TIMEOUT_MS: u64 = 45_000;
/// Marker prepended to fold summary content.
pub const COMPACTION_SUMMARY_MARKER: &str = "[compaction-summary]\n";

// ── Decision types ─────────────────────────────────────────────────────────

/// What to do after receiving API usage for a turn.
#[derive(Debug, Clone, PartialEq)]
pub enum PostUsageDecision {
    /// No action needed — context usage is within bounds.
    None {
        prompt_tokens: u64,
        ctx_max: usize,
        ratio: f64,
    },
    /// Fold the log (normal or aggressive).
    Fold {
        prompt_tokens: u64,
        ctx_max: usize,
        ratio: f64,
        tail_budget: usize,
        aggressive: bool,
    },
    /// Context is critically full — exit the turn with a summary message.
    ExitWithSummary {
        prompt_tokens: u64,
        ctx_max: usize,
        ratio: f64,
    },
}

/// Result of a fold operation.
#[derive(Debug, Clone)]
pub struct FoldResult {
    pub folded: bool,
    pub before_messages: usize,
    pub after_messages: usize,
    pub summary_chars: usize,
}

/// Turn-start estimate.
#[derive(Debug, Clone)]
pub struct TurnStartEstimate {
    pub estimate_tokens: usize,
    pub ctx_max: usize,
    pub ratio: f64,
}

// ── Decision logic ─────────────────────────────────────────────────────────

/// Decide what to do after receiving API usage for a completed iteration.
///
/// `provider` selects the context-window table so the ratio is measured
/// against the correct ceiling. Passing the thread's real provider keeps
/// non-DeepSeek threads from being measured against DeepSeek's 1M window —
/// a small-window model (e.g. 131K) would otherwise compute a tiny ratio,
/// never fold, and 400 on the next over-budget request. This mirrors
/// [`estimate_turn_start`] and satisfies the provider-neutrality rule that
/// mechanism-layer thresholds must be provider-correct
/// (see `.kiro/steering/provider-neutrality.md`).
///
/// `already_folded_this_turn`: prevents double-folding in the same turn.
pub fn decide_after_usage(
    usage: &Usage,
    model: &str,
    provider: ProviderId,
    already_folded_this_turn: bool,
) -> PostUsageDecision {
    let ctx_max = pricing::context_window(provider, model);
    let prompt_tokens = usage.prompt_tokens as u64;
    let ratio = prompt_tokens as f64 / ctx_max as f64;

    if ratio > FORCE_SUMMARY_THRESHOLD {
        return PostUsageDecision::ExitWithSummary {
            prompt_tokens,
            ctx_max,
            ratio,
        };
    }
    if already_folded_this_turn {
        return PostUsageDecision::None {
            prompt_tokens,
            ctx_max,
            ratio,
        };
    }
    if ratio > FOLD_AGGRESSIVE_THRESHOLD {
        return PostUsageDecision::Fold {
            prompt_tokens,
            ctx_max,
            ratio,
            tail_budget: (ctx_max as f64 * FOLD_AGGRESSIVE_TAIL_FRACTION) as usize,
            aggressive: true,
        };
    }
    if ratio > FOLD_THRESHOLD {
        return PostUsageDecision::Fold {
            prompt_tokens,
            ctx_max,
            ratio,
            tail_budget: (ctx_max as f64 * FOLD_TAIL_FRACTION) as usize,
            aggressive: false,
        };
    }
    PostUsageDecision::None {
        prompt_tokens,
        ctx_max,
        ratio,
    }
}

/// Estimate token usage at turn start (before calling the API).
/// Uses the local DeepSeek V4 tokenizer for an accurate (±5%) count via the
/// chat-template-aware conversation estimator.
///
/// `provider` selects the context-window table so the ratio is computed
/// against the correct ceiling — passing the real per-thread provider keeps
/// non-DeepSeek threads from being measured against DeepSeek's 1M window
/// (see `.kiro/steering/provider-neutrality.md`).
pub fn estimate_turn_start(
    messages: &[deepseek_client::types::ChatMessage],
    model: &str,
    provider: ProviderId,
) -> TurnStartEstimate {
    let ctx_max = pricing::context_window(provider, model);

    // Convert client messages → tokenizer messages so the estimate runs
    // through the real V4 chat template (per-message template overhead +
    // bounded BPE content counts), matching Reasonix `estimateRequestTokens`.
    //
    // Note: the BPE tables are DeepSeek V4's. For non-DeepSeek models the
    // token COUNT is approximate (their tokenizers differ), but it's a
    // reasonable threshold proxy and there is no per-provider tokenizer yet
    // (P3 spec §11 defers that). The context-window CEILING, however, is
    // provider-correct via the table above.
    let tok_messages: Vec<deepseek_tokenizer::ChatMessage> =
        messages.iter().map(to_tokenizer_message).collect();

    // `drop_thinking = true`: reasoning_content before the last user turn is
    // stripped by the template, matching what the API actually tokenizes.
    let estimate = deepseek_tokenizer::estimate_conversation_tokens(&tok_messages, true).max(1);

    TurnStartEstimate {
        estimate_tokens: estimate,
        ctx_max,
        ratio: estimate as f64 / ctx_max as f64,
    }
}

/// Convert a client `ChatMessage` into the tokenizer's minimal message type
/// for template-based token estimation.
fn to_tokenizer_message(
    m: &deepseek_client::types::ChatMessage,
) -> deepseek_tokenizer::ChatMessage {
    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
        tcs.iter()
            .map(|tc| deepseek_tokenizer::ToolCallEntry {
                id: tc.id.clone(),
                function: deepseek_tokenizer::ToolCallFunction {
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                },
            })
            .collect()
    });
    deepseek_tokenizer::ChatMessage {
        role: m.role.clone(),
        content: m.content_text().map(|s| deepseek_tokenizer::MessageContent::Text(s.to_string())),
        tool_calls,
        tool_call_id: m.tool_call_id.clone(),
        reasoning_content: m.reasoning_content.clone(),
    }
}

/// Determine the fold boundary: split messages into head (to summarize)
/// and tail (to keep). Returns `None` if fold is not worthwhile.
///
/// The boundary always falls on a `user` role message to avoid splitting
/// assistant+tool_calls / tool_result pairs.
pub fn find_fold_boundary(
    messages: &[deepseek_client::types::ChatMessage],
    tail_budget_tokens: usize,
) -> Option<usize> {
    if messages.is_empty() {
        return None;
    }

    // Walk from the end, accumulating token estimates per message.
    let mut cum_tokens: usize = 0;
    let mut boundary = messages.len();

    for i in (0..messages.len()).rev() {
        let msg = &messages[i];
        let msg_tokens = estimate_message_tokens(msg);
        if cum_tokens + msg_tokens > tail_budget_tokens {
            break;
        }
        cum_tokens += msg_tokens;
        // Boundary must fall on a user message
        if msg.role == "user" {
            boundary = i;
        }
    }

    if boundary == 0 || boundary >= messages.len() {
        return None;
    }

    // Check minimum savings: head must be at least 30% of total
    let total_tokens: usize = messages.iter().map(estimate_message_tokens).sum();
    let head_tokens = total_tokens - cum_tokens;
    if (head_tokens as f64) < (total_tokens as f64 * MIN_SAVINGS_FRACTION) {
        return None;
    }

    Some(boundary)
}

/// Per-message token estimate using the real V4 tokenizer.
///
/// Counts content + reasoning + tool_call argument JSON via bounded BPE
/// (sampling for very long fields), plus a flat template overhead. This is
/// what the fold boundary walk uses to keep the recent tail within budget —
/// accuracy matters here so heavy tool-call turns don't slip past the budget
/// and drag the boundary across an active tool turn.
fn estimate_message_tokens(msg: &deepseek_client::types::ChatMessage) -> usize {
    const PER_MESSAGE_TEMPLATE_OVERHEAD: usize = 6;
    let mut tokens = PER_MESSAGE_TEMPLATE_OVERHEAD;
    if let Some(c) = &msg.content {
        if let Some(text) = c.as_text() {
            tokens += deepseek_tokenizer::count_tokens_bounded(
                text,
                deepseek_tokenizer::DEFAULT_BOUNDED_TOKENIZE_CHARS,
            );
        }
    }

    if let Some(r) = &msg.reasoning_content {
        tokens += deepseek_tokenizer::count_tokens_bounded(
            r,
            deepseek_tokenizer::DEFAULT_BOUNDED_TOKENIZE_CHARS,
        );
    }
    if let Some(tcs) = &msg.tool_calls {
        for tc in tcs {
            tokens += deepseek_tokenizer::count_tokens_bounded(
                &tc.function.arguments,
                deepseek_tokenizer::DEFAULT_BOUNDED_TOKENIZE_CHARS,
            );
            tokens += deepseek_tokenizer::count_tokens_bounded(
                &tc.function.name,
                deepseek_tokenizer::DEFAULT_BOUNDED_TOKENIZE_CHARS,
            );
        }
    }
    tokens
}

/// Build the summary instruction appended to the head messages.
///
/// Structured after Claude Code's `BASE_COMPACT_PROMPT` (sectioned recap that
/// preserves objective, decisions, files, errors, and pending work) but
/// adapted to our fold contract: the output is a single self-contained prose
/// message (no `<analysis>`/`<summary>` XML round-trip, no tool calls), since
/// the summary replaces the folded head in-place in the conversation log.
pub fn build_fold_summary_instruction() -> String {
    "Summarize the conversation above into one self-contained recap that lets you continue \
     the work without the original messages. Be thorough on technical detail; skip \
     turn-by-turn play-by-play.\n\n\
     Cover, in order:\n\
     1. Original objective & constraints — the user's explicit goal, preserved VERBATIM. \
     Never paraphrase away negative constraints ('do NOT do X', 'never', 'avoid'); copy them as written.\n\
     2. Key decisions & technical context — approaches chosen, libraries, architecture, and why.\n\
     3. Files & code — files inspected, created, or modified, with the essential snippets/signatures needed to resume.\n\
     4. Errors & fixes — failures hit and how they were resolved; include any explicit user corrections.\n\
     5. Tool results still relevant — outputs that affect remaining work.\n\
     6. Pending work & next step — open todos and the immediate next action, in line with the user's latest request.\n\n\
     Output plain prose only — no tool calls, no markdown headings, no SEARCH/REPLACE blocks."
        .to_string()
}

/// Build the compaction summary message from summary text.
///
/// ## Provider neutrality
///
/// The `reasoning_content` placeholder is a DeepSeek thinking-mode
/// requirement: when thinking mode is active, every assistant message must
/// round-trip a `reasoning_content` field or the next request 400s. Other
/// providers have no such field — sending an empty placeholder there is at
/// best ignored and at worst rejected. So we only attach the placeholder
/// when `provider == Deepseek` (see
/// `.kiro/steering/provider-neutrality.md`). The default for every other
/// provider is a plain assistant message.
pub fn build_summary_message(
    summary: &str,
    provider: ProviderId,
) -> deepseek_client::types::ChatMessage {
    let reasoning_content = match provider {
        ProviderId::Deepseek => Some(String::new()),
        _ => None,
    };
    deepseek_client::types::ChatMessage {
        role: "assistant".to_string(),
        content: Some(deepseek_client::types::MessageContent::Text(format!("{COMPACTION_SUMMARY_MARKER}{summary}"))),
        reasoning_content,
        tool_calls: None,
        tool_call_id: None,
    }
}

// ── Fold summary request construction (cache-aligned) ──────────────────────

/// Build the message list for a fold summary call.
///
/// Cache-alignment is the whole point: the summary request reuses the main
/// agent's **verbatim** system prompt as `messages[0]`, then the head
/// messages to summarize, then a trailing user instruction. Because the
/// provider's prefix cache keys on the leading bytes, the system prompt (and
/// tools, passed via [`fold_summary_opts`]) hit the cache the main agent
/// already paid for — the summary call only pays cache-miss on the head.
///
/// Mirrors Reasonix `summarizeForFold`'s message assembly.
pub fn build_fold_summary_messages(
    system_prompt: &str,
    head: &[deepseek_client::types::ChatMessage],
    instruction: &str,
) -> Vec<deepseek_client::types::ChatMessage> {
    let mut msgs = Vec::with_capacity(head.len() + 2);
    msgs.push(deepseek_client::types::ChatMessage::system(system_prompt));
    msgs.extend_from_slice(head);
    msgs.push(deepseek_client::types::ChatMessage::user(instruction));
    msgs
}

/// Build the [`ChatOpts`] for a fold summary call.
///
/// ## Provider neutrality
///
/// Tools are forwarded verbatim for cache alignment (all providers).
/// `thinking: "disabled"` is a DeepSeek-only `extra_body` field — it's only
/// attached when `provider == Deepseek`; other providers get a plain request
/// (no `extra_body`), keeping them unaffected
/// (see `.kiro/steering/provider-neutrality.md`).
pub fn fold_summary_opts(
    tools: Vec<deepseek_client::types::ToolSpec>,
    provider: ProviderId,
) -> deepseek_client::deepseek::ChatOpts {
    let extra_body = match provider {
        ProviderId::Deepseek => Some(deepseek_client::types::ExtraBody {
            thinking: Some(deepseek_client::types::ThinkingConfig {
                thinking_type: "disabled".to_string(),
            }),
        }),
        _ => None,
    };
    deepseek_client::deepseek::ChatOpts {
        tools,
        extra_body,
        thinking: None,
        reasoning_effort: None,
    }
}

/// Assemble the post-summary replacement log: `[summary_msg, ...tail]`,
/// appending any messages that arrived after the pre-summary snapshot.
///
/// `all_snapshot_len` is the log length captured before the summary call;
/// `current` is the log at replacement time. If new messages landed during
/// the (awaited) summary call, they're preserved by appending the suffix
/// beyond `all_snapshot_len`. This keeps a fold from silently dropping a
/// message that raced in while the summary model was responding.
pub fn assemble_fold_replacement(
    summary_msg: deepseek_client::types::ChatMessage,
    tail: &[deepseek_client::types::ChatMessage],
    all_snapshot_len: usize,
    current: &[deepseek_client::types::ChatMessage],
) -> Vec<deepseek_client::types::ChatMessage> {
    let mut replacement = Vec::with_capacity(tail.len() + 2);
    replacement.push(summary_msg);
    replacement.extend_from_slice(tail);
    if current.len() > all_snapshot_len {
        replacement.extend_from_slice(&current[all_snapshot_len..]);
    }
    replacement
}

// ── ContextUsage event source ──────────────────────────────────────────────

/// Source of a ContextUsage measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextUsageSource {
    /// From API response `usage.prompt_tokens` (exact).
    Api,
    /// From local tokenizer estimate (approximate).
    Local,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_usage(prompt_tokens: u32) -> Usage {
        Usage {
            prompt_tokens,
            completion_tokens: 100,
            total_tokens: prompt_tokens + 100,
            prompt_cache_hit_tokens: 0,
            prompt_cache_miss_tokens: prompt_tokens,
        }
    }

    #[test]
    fn decide_none_when_low() {
        let decision = decide_after_usage(
            &make_usage(100_000),
            "deepseek-v4-flash",
            ProviderId::Deepseek,
            false,
        );
        assert!(matches!(decision, PostUsageDecision::None { .. }));
    }

    #[test]
    fn decide_fold_normal_at_threshold() {
        // 0.76 ratio → normal fold
        let decision = decide_after_usage(
            &make_usage(760_000),
            "deepseek-v4-flash",
            ProviderId::Deepseek,
            false,
        );
        match decision {
            PostUsageDecision::Fold { aggressive, .. } => assert!(!aggressive),
            _ => panic!("expected Fold, got {decision:?}"),
        }
    }

    #[test]
    fn decide_fold_aggressive_at_high_threshold() {
        // 0.79 ratio → aggressive fold
        let decision = decide_after_usage(
            &make_usage(790_000),
            "deepseek-v4-flash",
            ProviderId::Deepseek,
            false,
        );
        match decision {
            PostUsageDecision::Fold { aggressive, .. } => assert!(aggressive),
            _ => panic!("expected aggressive Fold, got {decision:?}"),
        }
    }

    #[test]
    fn decide_exit_with_summary_at_critical() {
        // 0.81 ratio → exit with summary
        let decision = decide_after_usage(
            &make_usage(810_000),
            "deepseek-v4-flash",
            ProviderId::Deepseek,
            false,
        );
        assert!(matches!(
            decision,
            PostUsageDecision::ExitWithSummary { .. }
        ));
    }

    #[test]
    fn decide_none_when_already_folded() {
        // Even at 0.76, if we already folded, don't fold again
        let decision = decide_after_usage(
            &make_usage(760_000),
            "deepseek-v4-flash",
            ProviderId::Deepseek,
            true,
        );
        assert!(matches!(decision, PostUsageDecision::None { .. }));
    }

    /// Regression (P1-1, provider neutrality): the SAME token count must be
    /// measured against the THREAD's provider window, not a hardcoded
    /// DeepSeek 1M. 200K tokens is ~20% of DeepSeek's 1M (no fold) but
    /// overflows a 131K-window non-DeepSeek model (must force exit/summary).
    /// Before the fix, `decide_after_usage` hardcoded `ProviderId::Deepseek`
    /// so the non-DeepSeek thread never folded and 400'd on the next call.
    #[test]
    fn decide_uses_thread_provider_window_not_hardcoded_deepseek() {
        let usage = make_usage(200_000);

        // DeepSeek 1M window: 200K = 0.2 ratio → no action.
        let ds = decide_after_usage(&usage, "deepseek-v4-flash", ProviderId::Deepseek, false);
        assert!(
            matches!(ds, PostUsageDecision::None { .. }),
            "200K on DeepSeek 1M should be None, got {ds:?}"
        );

        // Non-DeepSeek model falls back to the safe 131K window: 200K
        // already overflows it → must fold or exit (NOT None).
        let other = decide_after_usage(&usage, "some-131k-model", ProviderId::Other, false);
        assert!(
            !matches!(other, PostUsageDecision::None { .. }),
            "200K on a 131K-window provider must trigger fold/exit, got {other:?}"
        );
    }

    #[test]
    fn find_fold_boundary_returns_none_for_empty() {
        assert_eq!(find_fold_boundary(&[], 1000), None);
    }

    #[test]
    fn find_fold_boundary_lands_on_user() {
        use deepseek_client::types::ChatMessage;
        let msgs = vec![
            ChatMessage::user("q1"),
            ChatMessage::assistant("a1 is a longer response with some content"),
            ChatMessage::user("q2"),
            ChatMessage::assistant("a2"),
            ChatMessage::user("q3"),
            ChatMessage::assistant("a3"),
        ];
        // Small tail budget → boundary should be near the end on a user msg
        if let Some(boundary) = find_fold_boundary(&msgs, 50) {
            assert_eq!(msgs[boundary].role, "user");
        }
    }

    #[test]
    fn summary_message_has_marker() {
        let msg = build_summary_message("the recap", ProviderId::Deepseek);
        assert!(msg.content_text().unwrap().starts_with(COMPACTION_SUMMARY_MARKER));
    }

    /// Provider neutrality: DeepSeek summary carries an empty
    /// `reasoning_content` placeholder (thinking-mode round-trip requirement).
    #[test]
    fn summary_message_deepseek_has_reasoning_placeholder() {
        let msg = build_summary_message("recap", ProviderId::Deepseek);
        assert_eq!(msg.reasoning_content.as_deref(), Some(""));
    }

    /// Provider neutrality: non-DeepSeek summary is a plain assistant message
    /// with NO reasoning_content — other providers have no such field and
    /// could reject it.
    #[test]
    fn summary_message_other_provider_has_no_reasoning() {
        let msg = build_summary_message("recap", ProviderId::Other);
        assert_eq!(msg.reasoning_content, None);
    }

    #[test]
    fn estimate_turn_start_nonzero() {
        use deepseek_client::types::ChatMessage;
        let msgs = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello world"),
        ];
        let est = estimate_turn_start(&msgs, "deepseek-v4-flash", ProviderId::Deepseek);
        assert!(est.estimate_tokens > 0);
        assert_eq!(est.ctx_max, 1_000_000);
        assert!(est.ratio > 0.0);
        assert!(est.ratio < 0.01); // small conversation, tiny ratio
    }

    /// Provider neutrality: a non-DeepSeek provider with an unknown model
    /// must measure against the safe fallback window (131072), NOT
    /// DeepSeek's 1M — otherwise a non-DS thread could blow past its real
    /// ceiling before the fold ever triggers.
    #[test]
    fn estimate_turn_start_uses_provider_context_window() {
        use deepseek_client::types::ChatMessage;
        let msgs = vec![ChatMessage::user("Hello world")];
        let est = estimate_turn_start(&msgs, "mimo-v2.5", ProviderId::Other);
        assert_eq!(
            est.ctx_max, 131_072,
            "unknown provider falls back to safe window"
        );
    }

    // ── Cache-aligned summary request construction ─────────────────────────

    /// Cache alignment invariant: the summary request's first message MUST be
    /// the system prompt verbatim, so the provider's prefix cache hits the
    /// bytes the main agent already paid for.
    #[test]
    fn fold_summary_messages_start_with_system_verbatim() {
        use deepseek_client::types::ChatMessage;
        let system = "VERBATIM SYSTEM PROMPT with tools and rules";
        let head = vec![ChatMessage::user("q1"), ChatMessage::assistant("a1")];
        let msgs = build_fold_summary_messages(system, &head, "Summarize the above.");
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content_text(), Some(system));
        // head preserved in order
        assert_eq!(msgs[1].content_text(), Some("q1"));
        assert_eq!(msgs[2].content_text(), Some("a1"));
        // trailing instruction is a user message
        let last = msgs.last().unwrap();
        assert_eq!(last.role, "user");
        assert_eq!(last.content_text(), Some("Summarize the above."));
    }

    /// Provider neutrality: DeepSeek fold opts disable thinking via
    /// `extra_body.thinking`; tools are forwarded for cache alignment.
    #[test]
    fn fold_summary_opts_deepseek_disables_thinking() {
        let opts = fold_summary_opts(vec![], ProviderId::Deepseek);
        let eb = opts.extra_body.expect("deepseek sets extra_body");
        assert_eq!(eb.thinking.expect("thinking set").thinking_type, "disabled");
    }

    /// Provider neutrality: non-DeepSeek fold opts carry NO extra_body, so
    /// the summary request is a plain cross-provider chat completion.
    #[test]
    fn fold_summary_opts_other_provider_has_no_extra_body() {
        let opts = fold_summary_opts(vec![], ProviderId::Other);
        assert!(opts.extra_body.is_none());
    }

    // ── Replacement assembly ───────────────────────────────────────────────

    /// Normal case: replacement is [summary, ...tail] with no racing
    /// messages (current log == snapshot length).
    #[test]
    fn assemble_replacement_summary_then_tail() {
        use deepseek_client::types::ChatMessage;
        let summary = build_summary_message("recap", ProviderId::Deepseek);
        let tail = vec![
            ChatMessage::user("recent q"),
            ChatMessage::assistant("recent a"),
        ];
        // current == head(3) + tail(2) = 5; snapshot len was 5 (no race).
        let current_len = 5;
        let current: Vec<ChatMessage> = (0..current_len).map(|_| ChatMessage::user("x")).collect();
        let out = assemble_fold_replacement(summary, &tail, current_len, &current);
        assert_eq!(out.len(), 3, "summary + 2 tail");
        assert!(out[0]
            .content_text()
            .unwrap()
            .starts_with(COMPACTION_SUMMARY_MARKER));
        assert_eq!(out[1].content_text(), Some("recent q"));
        assert_eq!(out[2].content_text(), Some("recent a"));
    }

    /// Race case: a message landed during the summary call (current log is
    /// longer than the snapshot). The new suffix must be preserved.
    #[test]
    fn assemble_replacement_preserves_messages_that_raced_in() {
        use deepseek_client::types::ChatMessage;
        let summary = build_summary_message("recap", ProviderId::Deepseek);
        let tail = vec![ChatMessage::user("recent q")];
        let snapshot_len = 4;
        // Log grew to 6: indices [4],[5] arrived after the snapshot.
        let mut current: Vec<ChatMessage> = (0..snapshot_len)
            .map(|_| ChatMessage::user("old"))
            .collect();
        current.push(ChatMessage::assistant("raced-1"));
        current.push(ChatMessage::user("raced-2"));
        let out = assemble_fold_replacement(summary, &tail, snapshot_len, &current);
        // summary + 1 tail + 2 raced = 4
        assert_eq!(out.len(), 4);
        assert_eq!(out[2].content_text(), Some("raced-1"));
        assert_eq!(out[3].content_text(), Some("raced-2"));
    }
}
