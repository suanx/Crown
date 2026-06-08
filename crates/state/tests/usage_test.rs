//! UsageRepo integration tests.

use deepseek_state::{Database, ThreadInsert, ThreadRepo, UsageInsert, UsageRepo};
use tempfile::TempDir;

fn open_db() -> (TempDir, Database) {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("state.db");
    let db = Database::open(path).expect("open db");
    (tmp, db)
}

fn seed_thread(db: &Database, name: &str) -> String {
    let trepo = ThreadRepo::new(db);
    trepo
        .create(ThreadInsert {
            name: Some(name.into()),
            model: "deepseek-v4-flash".into(),
            cwd: None,
            permission_mode: "default".into(),
            provider_id: "deepseek".into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .expect("create thread")
        .id
}

#[test]
fn insert_and_thread_cost_basic() {
    let (_tmp, db) = open_db();
    let thread_id = seed_thread(&db, "t1");
    let urepo = UsageRepo::new(&db);

    urepo
        .insert(UsageInsert {
            thread_id: thread_id.clone(),
            message_id: "msg-1".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            cache_read_tokens: 1000,
            cache_miss_tokens: 5000,
            cache_creation_tokens: 0,
            output_tokens: 2000,
            cost_usd: 0.001_3,
            created_at: 1_000_000,
        })
        .unwrap();

    urepo
        .insert(UsageInsert {
            thread_id: thread_id.clone(),
            message_id: "msg-2".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            cache_read_tokens: 0,
            cache_miss_tokens: 1500,
            cache_creation_tokens: 0,
            output_tokens: 800,
            cost_usd: 0.000_434,
            created_at: 2_000_000,
        })
        .unwrap();

    let cost = urepo.thread_cost(&thread_id).unwrap();
    assert!((cost - 0.001_734).abs() < 1e-9, "cost = {cost}");
}

#[test]
fn total_since_filters_by_time() {
    let (_tmp, db) = open_db();
    let t1 = seed_thread(&db, "t1");
    let urepo = UsageRepo::new(&db);

    // Three rows at t = 1000, 5000, 9000.
    for (msg, ts, output) in &[("a", 1000_i64, 100_u64), ("b", 5000, 200), ("c", 9000, 300)] {
        urepo
            .insert(UsageInsert {
                thread_id: t1.clone(),
                message_id: (*msg).into(),
                provider_id: "deepseek".into(),
                model: "deepseek-v4-flash".into(),
                cache_read_tokens: 0,
                cache_miss_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: *output,
                cost_usd: (*output as f64) * 0.001,
                created_at: *ts,
            })
            .unwrap();
    }

    let lifetime = urepo.total_since(0).unwrap();
    assert_eq!(lifetime.output_tokens, 600);
    assert!((lifetime.total_cost_usd - 0.6).abs() < 1e-9);

    let mid = urepo.total_since(5000).unwrap();
    assert_eq!(mid.output_tokens, 500); // b + c
    let after_all = urepo.total_since(10_000).unwrap();
    assert_eq!(after_all.output_tokens, 0);
    assert_eq!(after_all.total_cost_usd, 0.0);
}

#[test]
fn empty_table_returns_zeros_not_error() {
    let (_tmp, db) = open_db();
    let urepo = UsageRepo::new(&db);

    let agg = urepo.total_since(0).unwrap();
    assert_eq!(agg.cache_read_tokens, 0);
    assert_eq!(agg.cache_miss_tokens, 0);
    assert_eq!(agg.cache_creation_tokens, 0);
    assert_eq!(agg.output_tokens, 0);
    assert_eq!(agg.total_cost_usd, 0.0);

    // thread_cost on a thread with no rows
    let t1 = seed_thread(&db, "t1");
    assert_eq!(urepo.thread_cost(&t1).unwrap(), 0.0);
}

#[test]
fn cache_read_breakdown_groups_by_provider_and_model() {
    let (_tmp, db) = open_db();
    let t1 = seed_thread(&db, "t1");
    let urepo = UsageRepo::new(&db);

    urepo
        .insert(UsageInsert {
            thread_id: t1.clone(),
            message_id: "a".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            cache_read_tokens: 1000,
            cache_miss_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            created_at: 100,
        })
        .unwrap();
    urepo
        .insert(UsageInsert {
            thread_id: t1.clone(),
            message_id: "b".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            cache_read_tokens: 500,
            cache_miss_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            created_at: 200,
        })
        .unwrap();
    urepo
        .insert(UsageInsert {
            thread_id: t1.clone(),
            message_id: "c".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            cache_read_tokens: 2000,
            cache_miss_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            created_at: 300,
        })
        .unwrap();

    let breakdown = urepo.cache_read_breakdown_since(0).unwrap();
    assert_eq!(breakdown.len(), 2);

    let flash_row = breakdown
        .iter()
        .find(|r| r.model == "deepseek-v4-flash")
        .expect("flash row");
    assert_eq!(flash_row.cache_read_tokens, 1500);
    assert_eq!(flash_row.provider_id, "deepseek");

    let pro_row = breakdown
        .iter()
        .find(|r| r.model == "deepseek-v4-pro")
        .expect("pro row");
    assert_eq!(pro_row.cache_read_tokens, 2000);
}

#[test]
fn cascade_delete_removes_usage_rows() {
    let (_tmp, db) = open_db();
    let t1 = seed_thread(&db, "t1");
    let urepo = UsageRepo::new(&db);

    urepo
        .insert(UsageInsert {
            thread_id: t1.clone(),
            message_id: "x".into(),
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            cache_read_tokens: 0,
            cache_miss_tokens: 100,
            cache_creation_tokens: 0,
            output_tokens: 50,
            cost_usd: 0.001,
            created_at: 100,
        })
        .unwrap();
    assert!((urepo.thread_cost(&t1).unwrap() - 0.001).abs() < 1e-9);

    let trepo = ThreadRepo::new(&db);
    trepo.delete(&t1).unwrap();

    // After cascade delete the per-thread sum is 0.
    assert_eq!(urepo.thread_cost(&t1).unwrap(), 0.0);
    let lifetime = urepo.total_since(0).unwrap();
    assert_eq!(lifetime.output_tokens, 0);
}
