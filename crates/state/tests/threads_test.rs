use deepseek_state::{Database, ThreadInsert, ThreadRepo, ThreadUpdate};
use tempfile::TempDir;

fn open_db() -> (TempDir, Database) {
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path().join("state.db")).unwrap();
    (tmp, db)
}

#[test]
fn create_and_list_threads() {
    let (_tmp, db) = open_db();
    let repo = ThreadRepo::new(&db);

    let t1 = repo
        .create(ThreadInsert {
            name: Some("first".into()),
            model: "deepseek-v4-flash".into(),
            cwd: Some("/tmp".into()),
            permission_mode: "default".into(),
            provider_id: "deepseek".into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .unwrap();
    assert_eq!(t1.name.as_deref(), Some("first"));

    // Sleep so t2's `updated_at` is strictly greater than t1's. Without this,
    // both creates can fall in the same millisecond on Windows; ORDER BY
    // updated_at DESC then ties and SQLite's tiebreaker is unspecified.
    std::thread::sleep(std::time::Duration::from_millis(2));

    let t2 = repo
        .create(ThreadInsert {
            name: None,
            model: "deepseek-v4-flash".into(),
            cwd: None,
            permission_mode: "default".into(),
            provider_id: "deepseek".into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .unwrap();

    let summaries = repo.list().unwrap();
    assert_eq!(summaries.len(), 2);
    // Ordered by updated_at DESC; t2 was created later
    assert_eq!(summaries[0].id, t2.id);
    assert_eq!(summaries[1].id, t1.id);
}

#[test]
fn update_thread_renames_and_pins() {
    let (_tmp, db) = open_db();
    let repo = ThreadRepo::new(&db);
    let t = repo.create(ThreadInsert::default()).unwrap();

    repo.update(
        &t.id,
        ThreadUpdate {
            name: Some(Some("renamed".into())),
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    let got = repo.get(&t.id).unwrap();
    assert_eq!(got.name.as_deref(), Some("renamed"));
    assert!(got.is_pinned);
}

#[test]
fn delete_thread_cascades_to_messages() {
    use deepseek_state::{MessageInsert, MessageRepo};
    let (_tmp, db) = open_db();
    let trepo = ThreadRepo::new(&db);
    let mrepo = MessageRepo::new(&db);
    let t = trepo.create(ThreadInsert::default()).unwrap();
    mrepo
        .append(MessageInsert {
            thread_id: t.id.clone(),
            seq: 0,
            role: "user".into(),
            content_json: "{}".into(),
        })
        .unwrap();
    trepo.delete(&t.id).unwrap();
    assert!(trepo.get(&t.id).is_err());
    let count = mrepo.count_for_thread(&t.id).unwrap();
    assert_eq!(count, 0, "messages should cascade");
}

#[test]
fn search_threads_by_title_and_preview() {
    let (_tmp, db) = open_db();
    let repo = ThreadRepo::new(&db);
    repo.create(ThreadInsert {
        name: Some("rust async".into()),
        ..Default::default()
    })
    .unwrap();
    repo.create(ThreadInsert {
        name: Some("python ML".into()),
        ..Default::default()
    })
    .unwrap();
    let hits = repo.search("rust").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name.as_deref(), Some("rust async"));
}

#[test]
fn list_hides_subagent_threads() {
    let (_tmp, db) = open_db();
    let repo = ThreadRepo::new(&db);

    let parent = repo
        .create(ThreadInsert {
            name: Some("parent".into()),
            model: "deepseek-v4-flash".into(),
            provider_id: "deepseek".into(),
            ..Default::default()
        })
        .unwrap();

    repo.create(ThreadInsert {
        name: Some("subagent".into()),
        model: "deepseek-v4-flash".into(),
        provider_id: "deepseek".into(),
        parent_thread_id: Some(parent.id.clone()),
        ..Default::default()
    })
    .unwrap();

    // Sidebar list shows only the top-level parent.
    let summaries = repo.list().unwrap();
    assert_eq!(summaries.len(), 1, "sub-agent thread must be hidden");
    assert_eq!(summaries[0].id, parent.id);

    // But the sub-agent thread is still retrievable by id.
    let sub = repo.get(&summaries[0].id).unwrap();
    assert!(sub.parent_thread_id.is_none());
}
