use deepseek_state::Database;
use tempfile::TempDir;

#[test]
fn opens_db_and_runs_migrations() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("state.db");

    // Open twice to verify migrations are idempotent
    let db1 = Database::open(&db_path).expect("first open");
    drop(db1);
    let db2 = Database::open(&db_path).expect("second open");

    // Verify all 4 tables exist
    let conn = db2.conn();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for expected in ["threads", "messages", "checkpoints", "usage"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing table {expected}, got: {names:?}"
        );
    }
}

#[test]
fn enables_wal_mode() {
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path().join("state.db")).unwrap();
    let mode: String = db
        .conn()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}
