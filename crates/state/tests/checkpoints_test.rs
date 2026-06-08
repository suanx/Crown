use deepseek_state::{CheckpointInsert, CheckpointRepo, Database, ThreadInsert, ThreadRepo};
use tempfile::TempDir;

fn setup() -> (TempDir, Database, String) {
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path().join("state.db")).unwrap();
    let t = ThreadRepo::new(&db)
        .create(ThreadInsert::default())
        .unwrap();
    (tmp, db, t.id)
}

#[test]
fn write_and_read_latest_checkpoint() {
    let (_tmp, db, tid) = setup();
    let repo = CheckpointRepo::new(&db);
    repo.put(CheckpointInsert {
        thread_id: tid.clone(),
        checkpoint_id: "latest".into(),
        state_json: r#"{"phase":"start"}"#.into(),
    })
    .unwrap();
    let got = repo
        .get(&tid, "latest")
        .unwrap()
        .expect("checkpoint exists");
    assert_eq!(got.state_json, r#"{"phase":"start"}"#);
}

#[test]
fn put_replaces_existing_checkpoint() {
    let (_tmp, db, tid) = setup();
    let repo = CheckpointRepo::new(&db);
    repo.put(CheckpointInsert {
        thread_id: tid.clone(),
        checkpoint_id: "latest".into(),
        state_json: "{\"v\":1}".into(),
    })
    .unwrap();
    repo.put(CheckpointInsert {
        thread_id: tid.clone(),
        checkpoint_id: "latest".into(),
        state_json: "{\"v\":2}".into(),
    })
    .unwrap();
    let got = repo.get(&tid, "latest").unwrap().unwrap();
    assert_eq!(got.state_json, "{\"v\":2}");
}

#[test]
fn delete_checkpoint() {
    let (_tmp, db, tid) = setup();
    let repo = CheckpointRepo::new(&db);
    repo.put(CheckpointInsert {
        thread_id: tid.clone(),
        checkpoint_id: "latest".into(),
        state_json: "{}".into(),
    })
    .unwrap();
    repo.delete(&tid, "latest").unwrap();
    assert!(repo.get(&tid, "latest").unwrap().is_none());
}

#[test]
fn list_threads_with_checkpoints_returns_only_threads_with_pending_recovery() {
    let (_tmp, db, tid_a) = setup();
    let trepo = ThreadRepo::new(&db);
    let _tid_b = trepo.create(ThreadInsert::default()).unwrap().id;
    let crepo = CheckpointRepo::new(&db);
    crepo
        .put(CheckpointInsert {
            thread_id: tid_a.clone(),
            checkpoint_id: "latest".into(),
            state_json: "{}".into(),
        })
        .unwrap();
    let pending = crepo.list_threads_with_checkpoints().unwrap();
    assert_eq!(pending, vec![tid_a]);
}
