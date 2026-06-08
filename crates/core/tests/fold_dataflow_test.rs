//! End-to-end data-flow test for context fold, minus the HTTP summary call.
//!
//! Proves the pieces the unit tests can't cover together: a realistic
//! conversation is split on a `user` boundary, the replacement
//! `[summary, ...tail]` is assembled, persisted via the transactional
//! `rewrite_thread`, and reloads from disk as the compacted log (so an LRU
//! eviction / restart sees the folded history, not the original).

use deepseek_client::types::ChatMessage;
use deepseek_core::compaction::{
    assemble_fold_replacement, build_summary_message, find_fold_boundary,
};
use deepseek_core::pricing::ProviderId;
use deepseek_state::{Database, MessageInsert, MessageRepo, ThreadInsert, ThreadRepo};
use tempfile::TempDir;

/// A long-ish synthetic conversation: alternating user/assistant turns, with
/// the early turns being the bulk of the tokens.
fn seed_conversation() -> Vec<ChatMessage> {
    let mut msgs = Vec::new();
    // 8 old turns (head candidate): big content so they dominate the budget.
    for i in 0..8 {
        msgs.push(ChatMessage::user(format!(
            "old question {i} {}",
            "context ".repeat(60)
        )));
        msgs.push(ChatMessage::assistant(format!(
            "old answer {i} {}",
            "detail ".repeat(60)
        )));
    }
    // 2 recent turns (tail): short.
    msgs.push(ChatMessage::user("recent question"));
    msgs.push(ChatMessage::assistant("recent answer"));
    msgs
}

#[test]
fn fold_dataflow_splits_assembles_and_persists() {
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path().join("state.db")).unwrap();
    let trepo = ThreadRepo::new(&db);
    let thread = trepo.create(ThreadInsert::default()).unwrap();
    let mrepo = MessageRepo::new(&db);

    let convo = seed_conversation();
    for (seq, m) in convo.iter().enumerate() {
        mrepo
            .append(MessageInsert {
                thread_id: thread.id.clone(),
                seq: seq as i64,
                role: m.role.clone(),
                content_json: serde_json::to_string(m).unwrap(),
            })
            .unwrap();
    }
    let before = mrepo.count_for_thread(&thread.id).unwrap();
    assert_eq!(before, convo.len() as u64);

    // A small tail budget so the boundary lands well before the end and the
    // head is large enough to clear the min-savings gate.
    let boundary = find_fold_boundary(&convo, 200).expect("a worthwhile boundary exists");
    // Boundary must land on a user message (never split a tool pair).
    assert_eq!(convo[boundary].role, "user", "boundary lands on user");
    assert!(boundary > 0 && boundary < convo.len());

    let tail = &convo[boundary..];
    let summary = build_summary_message("recap of the early turns", ProviderId::Deepseek);
    // No racing messages: snapshot len == current len.
    let replacement = assemble_fold_replacement(summary, tail, convo.len(), &convo);
    assert!(replacement.len() < convo.len(), "fold shrinks the log");
    assert!(
        replacement[0]
            .content
            .as_deref()
            .unwrap()
            .starts_with("[compaction-summary]\n"),
        "first message is the summary marker"
    );

    // Persist the rewrite and confirm it reloads as the compacted log.
    let rows: Vec<(String, String)> = replacement
        .iter()
        .map(|m| (m.role.clone(), serde_json::to_string(m).unwrap()))
        .collect();
    mrepo.rewrite_thread(&thread.id, rows).unwrap();

    let after = mrepo.count_for_thread(&thread.id).unwrap();
    assert_eq!(after, replacement.len() as u64, "disk reflects the fold");

    let reloaded = mrepo.load_by_thread(&thread.id).unwrap();
    assert_eq!(reloaded.len(), replacement.len());
    // Fresh contiguous 0-based seq so the next append (max_seq+1) won't collide.
    for (i, row) in reloaded.iter().enumerate() {
        assert_eq!(row.seq, i as i64);
    }
    // First reloaded row is the summary; last is the recent tail.
    let first: ChatMessage = serde_json::from_str(&reloaded[0].content_json).unwrap();
    assert!(first
        .content_text()
        .unwrap()
        .starts_with("[compaction-summary]\n"));
    let last: ChatMessage = serde_json::from_str(&reloaded.last().unwrap().content_json).unwrap();
    assert_eq!(last.content_text(), Some("recent answer"));
}

#[test]
fn fold_dataflow_noop_when_history_too_small() {
    // A 2-message conversation can't be meaningfully folded: the boundary
    // walk won't clear the min-savings gate, so no fold should happen.
    let convo = vec![ChatMessage::user("hi"), ChatMessage::assistant("hello")];
    assert_eq!(
        find_fold_boundary(&convo, 200),
        None,
        "tiny conversation yields no worthwhile boundary"
    );
}
