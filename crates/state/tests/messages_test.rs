use deepseek_state::{Database, MessageInsert, MessageRepo, ThreadInsert, ThreadRepo};
use tempfile::TempDir;

fn setup() -> (TempDir, Database, String) {
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path().join("state.db")).unwrap();
    let trepo = ThreadRepo::new(&db);
    let t = trepo.create(ThreadInsert::default()).unwrap();
    (tmp, db, t.id)
}

#[test]
fn append_and_load_messages_in_order() {
    let (_tmp, db, tid) = setup();
    let mrepo = MessageRepo::new(&db);
    for (seq, role) in [(0, "user"), (1, "assistant"), (2, "tool"), (3, "assistant")] {
        mrepo
            .append(MessageInsert {
                thread_id: tid.clone(),
                seq,
                role: role.into(),
                content_json: format!("{{\"x\":{seq}}}"),
            })
            .unwrap();
    }
    let rows = mrepo.load_by_thread(&tid).unwrap();
    assert_eq!(rows.len(), 4);
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.seq, i as i64);
    }
}

#[test]
fn append_with_duplicate_seq_fails() {
    let (_tmp, db, tid) = setup();
    let mrepo = MessageRepo::new(&db);
    mrepo
        .append(MessageInsert {
            thread_id: tid.clone(),
            seq: 0,
            role: "user".into(),
            content_json: "{}".into(),
        })
        .unwrap();
    let err = mrepo.append(MessageInsert {
        thread_id: tid,
        seq: 0,
        role: "user".into(),
        content_json: "{}".into(),
    });
    assert!(err.is_err(), "duplicate seq should be rejected");
}

#[test]
fn count_for_thread_returns_inserted_count() {
    let (_tmp, db, tid) = setup();
    let mrepo = MessageRepo::new(&db);
    for seq in 0..5 {
        mrepo
            .append(MessageInsert {
                thread_id: tid.clone(),
                seq,
                role: "user".into(),
                content_json: "{}".into(),
            })
            .unwrap();
    }
    assert_eq!(mrepo.count_for_thread(&tid).unwrap(), 5);
}

#[test]
fn rewrite_thread_replaces_all_messages_with_fresh_seq() {
    let (_tmp, db, tid) = setup();
    let mrepo = MessageRepo::new(&db);
    // Seed an existing 4-message history.
    for (seq, role) in [(0, "user"), (1, "assistant"), (2, "tool"), (3, "assistant")] {
        mrepo
            .append(MessageInsert {
                thread_id: tid.clone(),
                seq,
                role: role.into(),
                content_json: format!("{{\"old\":{seq}}}"),
            })
            .unwrap();
    }
    // Compaction produces a shorter replacement (summary + recent tail).
    let replacement = [("assistant", "{\"summary\":true}"), ("user", "{\"new\":1}")];
    mrepo
        .rewrite_thread(
            &tid,
            replacement
                .iter()
                .map(|(role, json)| (role.to_string(), json.to_string()))
                .collect(),
        )
        .unwrap();

    let rows = mrepo.load_by_thread(&tid).unwrap();
    assert_eq!(rows.len(), 2, "old history fully replaced");
    // Fresh 0-based contiguous seq so future appends via max_seq+1 don't collide.
    assert_eq!(rows[0].seq, 0);
    assert_eq!(rows[1].seq, 1);
    assert_eq!(rows[0].role, "assistant");
    assert!(rows[0].content_json.contains("summary"));
    assert_eq!(rows[1].role, "user");
    // max_seq reflects the rewrite so the next append is seq=2.
    assert_eq!(mrepo.max_seq(&tid).unwrap(), 1);
}

#[test]
fn rewrite_thread_only_touches_target_thread() {
    let (_tmp, db, tid) = setup();
    let trepo = ThreadRepo::new(&db);
    let other = trepo.create(ThreadInsert::default()).unwrap();
    let mrepo = MessageRepo::new(&db);
    mrepo
        .append(MessageInsert {
            thread_id: other.id.clone(),
            seq: 0,
            role: "user".into(),
            content_json: "{\"keep\":1}".into(),
        })
        .unwrap();
    mrepo
        .append(MessageInsert {
            thread_id: tid.clone(),
            seq: 0,
            role: "user".into(),
            content_json: "{\"old\":1}".into(),
        })
        .unwrap();
    mrepo
        .rewrite_thread(&tid, vec![("assistant".into(), "{\"new\":1}".into())])
        .unwrap();
    // Other thread untouched.
    assert_eq!(mrepo.count_for_thread(&other.id).unwrap(), 1);
    assert_eq!(mrepo.count_for_thread(&tid).unwrap(), 1);
}
