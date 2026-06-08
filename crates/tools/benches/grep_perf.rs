//! Micro-benchmarks for grep and glob tool performance.
//!
//! Run with: `cargo bench -p deepseek-tools`
//!
//! These measure search throughput on a realistic codebase-sized
//! directory tree — the deepseek-agent crate itself serves as the
//! test corpus.

use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;

/// Resolve the project root (the deepseek-agent workspace dir).
fn project_root() -> PathBuf {
    // The bench binary runs from target/release/deps/…, walk up to find
    // the Cargo workspace root where Cargo.toml with [workspace] lives.
    let mut root = std::env::current_dir().unwrap();
    loop {
        if root.join("Cargo.toml").exists() {
            // Check that this is the workspace root.
            let contents = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
            if contents.contains("[workspace]") {
                return root;
            }
        }
        if !root.pop() {
            // Fallback: use the crate's own dir for search corpus.
            return PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        }
    }
}

/// Walk all .rs files under `root`, collect their paths for the grep
/// run to traverse.
fn walk_rs_files(root: &PathBuf) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(false)
        .build()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .filter(|p| p.extension().map(|x| x == "rs").unwrap_or(false))
        .collect()
}

/// Count of .rs files in the workspace as a rough size metric.
fn bench_grep_corpus_size(c: &mut Criterion) {
    let root = project_root();
    let files = walk_rs_files(&root);
    let total_bytes: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum();
    c.bench_function("grep_corpus_size", |b| {
        b.iter(|| {
            black_box((files.len(), total_bytes));
        })
    });
}

/// Search for a common pattern across all Rust files in the workspace.
/// Equivalent to `rg pattern` without file-type filtering.
fn bench_grep_common_pattern(c: &mut Criterion) {
    let root = project_root();
    let files = walk_rs_files(&root);
    let pattern = "fn "; // every Rust function — guaranteed many matches.
    c.bench_function("grep_fn_pattern", |b| {
        b.iter(|| {
            let matcher = RegexMatcher::new(black_box(pattern)).unwrap();
            let mut searcher = SearcherBuilder::new().build();
            let mut count = 0usize;
            for path in &files {
                let result = std::fs::File::open(path);
                let Ok(file) = result else { continue };
                let mut sink = UTF8(|_ln, _matched| -> Result<bool, std::io::Error> {
                    count += 1;
                    Ok(true)
                });
                let _ = searcher.search_file(&matcher, &file, &mut sink);
            }
            black_box(count);
        })
    });
}

/// Search for a rare pattern: near-zero matches, worst-case for grep
/// since it must scan every file to completion.
fn bench_grep_rare_pattern(c: &mut Criterion) {
    let root = project_root();
    let files = walk_rs_files(&root);
    let pattern = "XYZZY_NONEXISTENT_PATTERN_98765";
    c.bench_function("grep_rare_pattern", |b| {
        b.iter(|| {
            let matcher = RegexMatcher::new(black_box(pattern)).unwrap();
            let mut searcher = SearcherBuilder::new().build();
            let mut count = 0usize;
            for path in &files {
                let Ok(file) = std::fs::File::open(path) else {
                    continue;
                };
                let mut sink = UTF8(|_ln, _matched| -> Result<bool, std::io::Error> {
                    count += 1;
                    Ok(true)
                });
                let _ = searcher.search_file(&matcher, &file, &mut sink);
            }
            black_box(count);
        })
    });
}

/// Walk the directory tree (glob-style) without any regex matching.
/// Measures filesystem traversal throughput.
fn bench_glob_walk(c: &mut Criterion) {
    let root = project_root();
    c.bench_function("glob_walk_all", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for entry in WalkBuilder::new(black_box(&root))
                .hidden(false)
                .git_ignore(false)
                .build()
                .filter_map(|e| e.ok())
            {
                black_box(entry.path());
                count += 1;
            }
            black_box(count);
        })
    });
}

/// Case-insensitive regex search (forces different internal code path in
/// the grep-searcher library).
fn bench_grep_case_insensitive(c: &mut Criterion) {
    let root = project_root();
    let files = walk_rs_files(&root);
    let pattern = "(?i)error";
    c.bench_function("grep_case_insensitive", |b| {
        b.iter(|| {
            let matcher = RegexMatcher::new(black_box(pattern)).unwrap();
            let mut searcher = SearcherBuilder::new().build();
            let mut count = 0usize;
            for path in &files {
                let Ok(file) = std::fs::File::open(path) else {
                    continue;
                };
                let mut sink = UTF8(|_ln, _matched| -> Result<bool, std::io::Error> {
                    count += 1;
                    Ok(true)
                });
                let _ = searcher.search_file(&matcher, &file, &mut sink);
            }
            black_box(count);
        })
    });
}

criterion_group!(
    benches,
    bench_grep_corpus_size,
    bench_grep_common_pattern,
    bench_grep_rare_pattern,
    bench_glob_walk,
    bench_grep_case_insensitive,
);
criterion_main!(benches);
