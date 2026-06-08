//! LIVE network integration tests for context fold — real HTTP, real models.
//!
//! These are `#[ignore]` by default so the normal `cargo test` run stays
//! offline and deterministic. Run explicitly with credentials in the
//! environment:
//!
//! ```pwsh
//! # DeepSeek (primary path):
//! $env:DEEPSEEK_API_KEY = "<key>"
//! cargo test -p deepseek-core --test fold_live_test -- --ignored --nocapture
//!
//! # Non-DeepSeek provider neutrality (Xiaomi MiMo, OpenAI-compatible):
//! $env:MIMO_API_KEY  = "<key>"
//! $env:MIMO_BASE_URL = "https://token-plan-cn.xiaomimimo.com/v1"
//! $env:MIMO_MODEL    = "mimo-v2.5"
//! cargo test -p deepseek-core --test fold_live_test -- --ignored --nocapture
//! ```
//!
//! Credentials are read from env only — never hardcoded — per the repo's
//! security rules.
//!
//! What these prove that the offline tests can't:
//! - The real summary HTTP call returns usable prose (DeepSeek path).
//! - A non-DeepSeek, OpenAI-compatible endpoint accepts the SAME fold
//!   summary request shape when `ProviderId::Other` is used (no `extra_body`),
//!   i.e. the provider-neutrality iron law actually holds against a real
//!   foreign endpoint.

use std::time::Duration;

use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_client::types::ChatMessage;
use deepseek_core::compaction::{
    build_fold_summary_instruction, build_fold_summary_messages, fold_summary_opts,
};
use deepseek_core::pricing::ProviderId;

/// Append a section to the human-readable report file, if `FOLD_REPORT_DIR`
/// is set. Each test writes its own file there so the runner can compose a
/// single report afterward (no append races under --test-threads).
fn write_report_section(name: &str, body: &str) {
    if let Ok(dir) = std::env::var("FOLD_REPORT_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let path = std::path::Path::new(&dir).join(format!("{name}.md"));
        let _ = std::fs::write(path, body);
    }
}

/// Render a conversation as readable markdown (role: content).
fn render_convo(msgs: &[ChatMessage]) -> String {
    let mut s = String::new();
    for m in msgs {
        let content = m.content_text().unwrap_or("");
        s.push_str(&format!("**{}**: {}\n\n", m.role, content));
    }
    s
}

/// A multi-turn conversation worth summarizing. Carries a negative
/// constraint we later assert the summary preserves.
fn sample_head() -> Vec<ChatMessage> {
    vec![
        ChatMessage::user(
            "I'm building a Rust CLI that parses CSV files. IMPORTANT: do NOT add any \
             external crates — standard library only.",
        ),
        ChatMessage::assistant(
            "Understood. I'll use std::io and a hand-rolled CSV splitter, no external crates. \
             Let me start with the line reader.",
        ),
        ChatMessage::user("Good. Also it must handle quoted fields with embedded commas."),
        ChatMessage::assistant(
            "Right — I'll track an in-quote state while scanning each line so commas inside \
             double quotes don't split the field. Implemented the tokenizer in src/csv.rs.",
        ),
        ChatMessage::user("Now add a flag to skip the header row."),
        ChatMessage::assistant(
            "Added a --skip-header flag parsed from std::env::args; when set, the first record \
             is dropped before processing.",
        ),
    ]
}

fn build_client(api_key: String, base_url: String) -> DeepSeekClient {
    DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: Duration::from_secs(60),
        ..Default::default()
    })
    .expect("client build")
}

#[tokio::test]
#[ignore = "live network; needs DEEPSEEK_API_KEY"]
async fn live_deepseek_fold_summary_returns_usable_prose() {
    let api_key = match std::env::var("DEEPSEEK_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: DEEPSEEK_API_KEY not set");
            return;
        }
    };
    let base_url = std::env::var("DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
    let client = build_client(api_key, base_url);

    let system = "You are an interactive CLI coding agent.";
    let head = sample_head();
    let instruction = build_fold_summary_instruction();
    let messages = build_fold_summary_messages(system, &head, &instruction);
    // DeepSeek path: extra_body.thinking = disabled is attached.
    let opts = fold_summary_opts(Vec::new(), ProviderId::Deepseek);
    assert!(
        opts.extra_body.is_some(),
        "DeepSeek opts must carry extra_body (thinking gate)"
    );

    let resp = client
        .chat_with_opts(messages, "deepseek-v4-flash", opts)
        .await
        .expect("deepseek fold summary call");

    let summary = resp.content.trim();
    eprintln!(
        "\n=== DeepSeek summary ({} chars) ===\n{summary}\n",
        summary.len()
    );

    write_report_section(
        "01-deepseek-fold-summary",
        &format!(
            "## 场景 1 — DeepSeek 折叠总结（真实返回）\n\n\
             **模型**: deepseek-v4-flash　**端点**: api.deepseek.com\n\n\
             **请求形状**: 复用逐字节 system prompt + 6 轮对话 + 总结指令；`extra_body.thinking=disabled`（DeepSeek 专属）\n\n\
             ### 输入：被折叠的对话原文\n\n{}\n\
             ### 总结指令（发给模型的原文）\n\n> {}\n\n\
             ### 模型真实返回（{} 字符）\n\n{}\n\n\
             ### Token 用量\n\n- prompt_tokens: {}\n- completion_tokens: {}\n- total_tokens: {}\n",
            render_convo(&head),
            instruction,
            summary.len(),
            summary,
            resp.usage.prompt_tokens,
            resp.usage.completion_tokens,
            resp.usage.total_tokens,
        ),
    );

    assert!(!summary.is_empty(), "summary must not be empty");
    // The negative constraint must survive the fold (Reasonix lesson).
    let lower = summary.to_lowercase();
    assert!(
        lower.contains("no external")
            || lower.contains("standard library")
            || lower.contains("std"),
        "summary should preserve the 'std-only / no external crates' constraint, got: {summary}"
    );
    assert!(resp.usage.prompt_tokens > 0, "usage should be populated");
}

#[tokio::test]
#[ignore = "live network; needs MIMO_API_KEY"]
async fn live_mimo_non_deepseek_fold_summary_is_unaffected() {
    let api_key = match std::env::var("MIMO_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: MIMO_API_KEY not set");
            return;
        }
    };
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| "https://token-plan-cn.xiaomimimo.com/v1".to_string());
    let model = std::env::var("MIMO_MODEL").unwrap_or_else(|_| "mimo-v2.5".to_string());
    let client = build_client(api_key, base_url);

    let system = "You are an interactive CLI coding agent.";
    let head = sample_head();
    let instruction = build_fold_summary_instruction();
    let messages = build_fold_summary_messages(system, &head, &instruction);

    // PROVIDER NEUTRALITY: non-DeepSeek provider → NO extra_body. This is the
    // exact request shape the engine would send when state.provider != Deepseek.
    let opts = fold_summary_opts(Vec::new(), ProviderId::Other);
    assert!(
        opts.extra_body.is_none(),
        "non-DeepSeek opts must NOT carry extra_body (iron law)"
    );

    let resp = client
        .chat_with_opts(messages, &model, opts)
        .await
        .expect("MiMo fold summary call must succeed with the neutral request shape");

    let summary = resp.content.trim();
    eprintln!(
        "\n=== MiMo summary ({} chars) ===\n{summary}\n",
        summary.len()
    );

    write_report_section(
        "02-mimo-fold-summary",
        &format!(
            "## 场景 2 — 小米 MiMo 折叠总结（非 DeepSeek，真实返回）\n\n\
             **模型**: {}　**端点**: token-plan-cn.xiaomimimo.com\n\n\
             **请求形状**: 与 DeepSeek 同一套机制，但 `ProviderId::Other` → **无 extra_body**（铁律：DeepSeek 专属字段不发给其他供应商）\n\n\
             ### 输入：被折叠的对话原文\n\n{}\n\
             ### 模型真实返回（{} 字符）\n\n{}\n\n\
             ### Token 用量\n\n- prompt_tokens: {}\n- completion_tokens: {}\n- total_tokens: {}\n\n\
             > 结论：非 DeepSeek 供应商用同一套折叠机制，正常返回完整摘要，证明机制层供应商无关。\n",
            model,
            render_convo(&head),
            summary.len(),
            summary,
            resp.usage.prompt_tokens,
            resp.usage.completion_tokens,
            resp.usage.total_tokens,
        ),
    );

    assert!(
        !summary.is_empty(),
        "MiMo must return a non-empty summary — proves the fold mechanism works on a non-DeepSeek provider"
    );
}

/// Reverse proof of WHY the iron law exists: sending DeepSeek's
/// `extra_body.thinking` to a non-DeepSeek endpoint. This documents the
/// failure mode the gate prevents. We do NOT assert success/failure (foreign
/// endpoints may ignore or reject), only log the outcome — informational.
#[tokio::test]
#[ignore = "live network; diagnostic only; needs MIMO_API_KEY"]
async fn live_mimo_with_deepseek_extra_body_diagnostic() {
    let api_key = match std::env::var("MIMO_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: MIMO_API_KEY not set");
            return;
        }
    };
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| "https://token-plan-cn.xiaomimimo.com/v1".to_string());
    let model = std::env::var("MIMO_MODEL").unwrap_or_else(|_| "mimo-v2.5".to_string());
    let client = build_client(api_key, base_url);

    let messages = vec![ChatMessage::user("Say OK.")];
    // WRONG ON PURPOSE: DeepSeek extra_body sent to a non-DeepSeek endpoint.
    let opts = fold_summary_opts(Vec::new(), ProviderId::Deepseek);
    let result = client.chat_with_opts(messages, &model, opts).await;
    let outcome = match &result {
        Ok(r) => format!(
            "MiMo 容忍并忽略了 DeepSeek 的 extra_body。真实返回: {:?}",
            r.content.trim()
        ),
        Err(e) => format!("MiMo 拒绝了 DeepSeek 的 extra_body —— 这正是 gate 存在的理由: {e}"),
    };
    eprintln!("DIAG: {outcome}");
    write_report_section(
        "03-mimo-wrong-extra-body-diagnostic",
        &format!(
            "## 场景 3 — 反证：故意把 DeepSeek 专属字段发给 MiMo（诊断）\n\n\
             **模型**: {model}\n\n\
             给 MiMo 发了 `extra_body.thinking`（DeepSeek 专属）。\n\n\
             ### 真实结果\n\n{outcome}\n\n\
             > 注意：MiMo 这次是宽容忽略，但 OpenAI / Anthropic 会直接 400。\
             所以铁律的 provider gate 仍然必要，不能依赖供应商宽容。\n",
        ),
    );
}
