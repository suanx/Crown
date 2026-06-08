//! Performance baselines for the state crate.
//!
//! Targets per `docs/superpowers/specs/2026-05-28-p4-state-permission-design.md`
//! §12 ("性能目标"):
//!
//! - `list_threads` over 1000 rows: <50ms
//! - `message_append`: <1ms
//!
//! Run with: `cargo bench -p deepseek-state`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use deepseek_state::{Database, MessageInsert, MessageRepo, ThreadInsert, ThreadRepo};
use tempfile::TempDir;

fn bench_list_threads_1000(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(tmp.path().join("perf.db")).expect("open db");
    let repo = ThreadRepo::new(&db);
    for i in 0..1000 {
        repo.create(ThreadInsert {
            name: Some(format!("t{i}")),
            ..Default::default()
        })
        .expect("create");
    }
    c.bench_function("list_threads_1000", |b| {
        b.iter(|| {
            let v = repo.list().expect("list");
            black_box(v);
        });
    });
}

fn bench_message_append(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    let db = Database::open(tmp.path().join("perf.db")).expect("open db");
    let trepo = ThreadRepo::new(&db);
    let mrepo = MessageRepo::new(&db);
    let t = trepo
        .create(ThreadInsert::default())
        .expect("create thread");
    // Pre-bound counter so `iter_with_setup` doesn't dominate the timing.
    let mut seq: i64 = 0;
    c.bench_function("message_append", |b| {
        b.iter(|| {
            mrepo
                .append(MessageInsert {
                    thread_id: t.id.clone(),
                    seq,
                    role: "user".into(),
                    content_json: "{\"x\":1}".into(),
                })
                .expect("append");
            seq += 1;
        });
    });
}

criterion_group!(benches, bench_list_threads_1000, bench_message_append);
criterion_main!(benches);
