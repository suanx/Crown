//! Rewind (P2) integration test: file restoration + message truncation.

use std::sync::Arc;

use deepseek_core::rewind::{rewind_thread, DbFileHistorySink};
use deepseek_state::{Database, FileHistoryRepo, MessageInsert, MessageRepo};
use deepseek_tools::FileHistorySink;

fn seed_thread(db: &Database, id: &str) {
    db.conn()
        .execute(
            "INSERT INTO threads (id, model, created_at, updated_at) VALUES (?1,'m',0,0)",
            [id],
        )
        .unwrap();
}

#[tokio::test]
async fn rewind_restores_files_and_truncates_messages() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(dir.path().join("state.db")).unwrap());
    seed_thread(&db, "t");

    // Messages: seq 0 (user), 1 (assistant), 2 (user), 3 (assistant).
    let mrepo = MessageRepo::new(&db);
    for (seq, role) in [(0, "user"), (1, "assistant"), (2, "user"), (3, "assistant")] {
        mrepo
            .append(MessageInsert {
                thread_id: "t".into(),
                seq,
                role: role.into(),
                content_json: format!("{{\"role\":\"{role}\",\"seq\":{seq}}}"),
            })
            .unwrap();
    }

    // A real file that gets modified at turn seq=2, and a new file created at seq=2.
    let existing = dir.path().join("existing.txt");
    let created = dir.path().join("created.txt");
    tokio::fs::write(&existing, "ORIGINAL").await.unwrap();

    // Record history as the sink would during the turn anchored at seq=2.
    let sink = DbFileHistorySink::new(db.clone());
    sink.record("t", 2, existing.to_str().unwrap(), Some("ORIGINAL".into()));
    sink.record("t", 2, created.to_str().unwrap(), None); // didn't exist before

    // Simulate the turn's writes.
    tokio::fs::write(&existing, "MODIFIED").await.unwrap();
    tokio::fs::write(&created, "NEW FILE").await.unwrap();

    // Sanity: files are in post-change state.
    assert_eq!(
        tokio::fs::read_to_string(&existing).await.unwrap(),
        "MODIFIED"
    );
    assert!(created.exists());

    // Rewind to seq=2 (drop the seq>=2 turn).
    let outcome = rewind_thread(db.clone(), "t", 2).await.unwrap();

    // Files restored: existing back to ORIGINAL, created removed.
    assert_eq!(
        tokio::fs::read_to_string(&existing).await.unwrap(),
        "ORIGINAL"
    );
    assert!(!created.exists(), "newly-created file should be removed");
    assert_eq!(outcome.files_restored, 2);

    // Messages truncated: seq 0,1 remain.
    let rows = mrepo.load_by_thread("t").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.last().unwrap().seq, 1);
    assert_eq!(outcome.messages_removed, 2);

    // History rows cleared.
    assert_eq!(FileHistoryRepo::new(&db).since("t", 2).unwrap().len(), 0);
}

#[tokio::test]
async fn rewind_restores_to_earliest_before_when_file_edited_twice() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(dir.path().join("state.db")).unwrap());
    seed_thread(&db, "t");

    let f = dir.path().join("f.txt");
    tokio::fs::write(&f, "V0").await.unwrap();

    let sink = DbFileHistorySink::new(db.clone());
    // Two edits in the same turn (seq=1): V0→V1, then V1→V2.
    sink.record("t", 1, f.to_str().unwrap(), Some("V0".into()));
    tokio::fs::write(&f, "V1").await.unwrap();
    sink.record("t", 1, f.to_str().unwrap(), Some("V1".into()));
    tokio::fs::write(&f, "V2").await.unwrap();

    rewind_thread(db.clone(), "t", 1).await.unwrap();

    // Must restore to the EARLIEST before (V0), not V1.
    assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "V0");
}

/// Multi-file rewind across two turns: turn 1 (seq=1) modifies file_a,
/// turn 2 (seq=3) modifies file_b and creates file_c. Rewind to seq=3
/// should only undo turn 2's changes, leaving turn 1's intact.
#[tokio::test]
async fn rewind_multi_file_multi_turn_partial() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(dir.path().join("state.db")).unwrap());
    seed_thread(&db, "t");

    // Messages: seq 0 (user), 1 (assistant, turn1), 2 (user, turn2), 3 (assistant, turn2)
    let mrepo = MessageRepo::new(&db);
    for (seq, role) in [(0, "user"), (1, "assistant"), (2, "user"), (3, "assistant")] {
        mrepo
            .append(MessageInsert {
                thread_id: "t".into(),
                seq,
                role: role.into(),
                content_json: format!("{{\"role\":\"{role}\",\"seq\":{seq}}}"),
            })
            .unwrap();
    }

    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    let file_c = dir.path().join("c.txt");

    // Turn 1 (seq=1): modify file_a.
    tokio::fs::write(&file_a, "A_V0").await.unwrap();
    let sink = DbFileHistorySink::new(db.clone());
    sink.record("t", 1, file_a.to_str().unwrap(), Some("A_V0".into()));
    tokio::fs::write(&file_a, "A_V1").await.unwrap();

    // Turn 2 (seq=3): modify file_b and create file_c.
    tokio::fs::write(&file_b, "B_V0").await.unwrap();
    sink.record("t", 3, file_b.to_str().unwrap(), Some("B_V0".into()));
    tokio::fs::write(&file_b, "B_V1").await.unwrap();
    sink.record("t", 3, file_c.to_str().unwrap(), None);
    tokio::fs::write(&file_c, "NEW").await.unwrap();

    // Rewind to seq=3: this removes messages seq>=3 (only seq=3, the
    // turn-2 assistant) and restores files changed at/after seq=3.
    // seq=2 (the turn-2 user message) is preserved — rewind doesn't
    // delete user messages, it deletes from the assistant seq onward.
    // This matches the rewind contract: "roll back to (and excluding)
    // message_seq".
    let outcome = rewind_thread(db.clone(), "t", 3).await.unwrap();

    // Turn 1 changes STAY.
    assert_eq!(tokio::fs::read_to_string(&file_a).await.unwrap(), "A_V1");
    // Turn 2 changes REVERTED.
    assert_eq!(tokio::fs::read_to_string(&file_b).await.unwrap(), "B_V0");
    assert!(!file_c.exists());
    assert_eq!(outcome.files_restored, 2);

    // Messages: seq 0,1,2 remain (seq 2 is the user message for turn 2;
    // only seq 3 was deleted). messages_removed counts the single
    // assistant message at seq=3 plus file-history-triggered messages.
    let rows = mrepo.load_by_thread("t").unwrap();
    let remaining: Vec<i64> = rows.iter().map(|r| r.seq).collect();
    assert!(
        remaining.contains(&0),
        "seq 0 should remain, got {:?}",
        remaining
    );
    assert!(
        remaining.contains(&1),
        "seq 1 should remain, got {:?}",
        remaining
    );
    assert!(
        remaining.contains(&2),
        "seq 2 (user msg) should remain, got {:?}",
        remaining
    );
    assert_eq!(outcome.messages_removed, 1);
}

/// Rewind with interrupted turn: some files snapshot but never written,
/// others written. The rewind must handle both paths gracefully.
#[tokio::test]
async fn rewind_interrupted_turn_partial_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(dir.path().join("state.db")).unwrap());
    seed_thread(&db, "t");

    let mrepo = MessageRepo::new(&db);
    mrepo
        .append(MessageInsert {
            thread_id: "t".into(),
            seq: 0,
            role: "user".into(),
            content_json: r#"{"role":"user","seq":0}"#.into(),
        })
        .unwrap();

    let f1 = dir.path().join("snapshot_only.txt"); // snapshotted but never written
    let f2 = dir.path().join("actually_changed.txt");
    tokio::fs::write(&f1, "f1_old").await.unwrap();
    tokio::fs::write(&f2, "f2_old").await.unwrap();

    let sink = DbFileHistorySink::new(db.clone());
    sink.record("t", 1, f1.to_str().unwrap(), Some("f1_old".into()));
    // f1 never gets a new write (interrupted before the tool ran).
    sink.record("t", 1, f2.to_str().unwrap(), Some("f2_old".into()));
    tokio::fs::write(&f2, "f2_new").await.unwrap();

    rewind_thread(db.clone(), "t", 1).await.unwrap();

    // f1: snapshot says "f1_old" → restore to "f1_old" (idempotent).
    assert_eq!(tokio::fs::read_to_string(&f1).await.unwrap(), "f1_old");
    // f2: changed → restored.
    assert_eq!(tokio::fs::read_to_string(&f2).await.unwrap(), "f2_old");
}
