//! Read-before-write file state cache.
//!
//! Mirrors Claude Code's `src/utils/fileStateCache.ts` (`FileState` + the
//! `readFileState` Map). This is the backbone of the Read → Edit/Write
//! safety loop: a tool that mutates a file consults this cache to verify the
//! model has actually read the file and that it hasn't changed on disk since.
//!
//! Keys are canonicalized so `./foo.rs` (relative) and `/abs/foo.rs`
//! (absolute) resolve to the same entry — otherwise a Read of one form and an
//! Edit of the other would never match and edits would always be rejected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Snapshot of a file at the time it was last Read (or Written).
///
/// `content` holds the LF-normalized bytes the model has seen. `timestamp`
/// is the file modification time (unix ms) captured at read/write time and is
/// what freshness checks compare against. `offset`/`limit` record the visible
/// line range — `None` means a full read (set by Read; Edit/Write store
/// `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    /// File content the model has seen (CRLF normalized to LF).
    pub content: String,
    /// Modification time (unix ms) at read/write time.
    pub timestamp: i64,
    /// Read offset (0-indexed start line); `None` for full reads and writes.
    pub offset: Option<usize>,
    /// Read limit (line count); `None` when unbounded.
    pub limit: Option<usize>,
}

impl FileState {
    /// `true` when this entry reflects a full-file read (no offset/limit).
    /// Edits are only allowed against full reads; partial views must be
    /// re-read fully before mutation.
    pub fn is_full_read(&self) -> bool {
        self.offset.is_none() && self.limit.is_none()
    }
}

/// Path-keyed cache of [`FileState`] for read-before-write enforcement.
///
/// One instance lives per thread (in `ThreadState`) and survives across
/// turns, matching Claude's session-scoped `readFileState`.
#[derive(Debug, Default)]
pub struct FileStateCache {
    states: HashMap<String, FileState>,
}

impl FileStateCache {
    /// Construct an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop all recorded file states. Called after a context fold rewrites
    /// the conversation log: the read-before-edit snapshots referred to
    /// reads that are no longer present in the (now-summarized) history, so
    /// they must not authorize edits against vanished context.
    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Look up the recorded state for `path` (canonicalized internally).
    pub fn get_normalized(&self, normalized_key: &str) -> Option<&FileState> {
        self.states.get(normalized_key)
    }

    /// Look up state for a raw path given the working directory.
    pub fn get(&self, path: &str, cwd: Option<&Path>) -> Option<&FileState> {
        let key = Self::normalize_key(path, cwd);
        self.states.get(&key)
    }

    /// Record a Read. Stores the visible content + the read range so the
    /// caller can later distinguish full vs partial views.
    pub fn record_read(
        &mut self,
        path: &str,
        cwd: Option<&Path>,
        content: String,
        timestamp: i64,
        offset: Option<usize>,
        limit: Option<usize>,
    ) {
        let key = Self::normalize_key(path, cwd);
        self.states.insert(
            key,
            FileState {
                content,
                timestamp,
                offset,
                limit,
            },
        );
    }

    /// Record a Write/Edit. Marks the entry as a full view (offset/limit
    /// `None`) so a subsequent edit doesn't trip the partial-view guard, and
    /// refreshes the timestamp to invalidate stale-write detection.
    pub fn record_write(
        &mut self,
        path: &str,
        cwd: Option<&Path>,
        content: String,
        timestamp: i64,
    ) {
        let key = Self::normalize_key(path, cwd);
        self.states.insert(
            key,
            FileState {
                content,
                timestamp,
                offset: None,
                limit: None,
            },
        );
    }

    /// Canonicalize a path for use as a cache key.
    ///
    /// Steps:
    /// 1. Resolve relative paths against `cwd` (if provided).
    /// 2. Normalize separators to `/`.
    /// 3. Lowercase a Windows drive letter prefix (`C:` → `c:`) so
    ///    case-variant drive specs collide.
    ///
    /// Note: this is a string-level normalization — it does NOT resolve
    /// symlinks or `..` segments via the filesystem (matching Claude's
    /// expandPath-then-key approach, which avoids ENOENT on not-yet-created
    /// files).
    pub fn normalize_key(path: &str, cwd: Option<&Path>) -> String {
        // Resolve relative paths against cwd.
        let pb = PathBuf::from(path);
        let absolute: PathBuf = if pb.is_absolute() {
            pb
        } else if let Some(base) = cwd {
            base.join(pb)
        } else {
            pb
        };

        // Separator normalization.
        let mut s = absolute.to_string_lossy().replace('\\', "/");

        // Collapse "/./" segments (cheap, common case from cwd.join("./x")).
        while let Some(idx) = s.find("/./") {
            s.replace_range(idx..idx + 2, "");
        }
        if let Some(stripped) = s.strip_prefix("./") {
            s = stripped.to_string();
        }

        // Lowercase Windows drive letter (e.g. "C:/..." -> "c:/...").
        if s.len() >= 2 && s.as_bytes()[1] == b':' && s.as_bytes()[0].is_ascii_alphabetic() {
            let mut chars: Vec<char> = s.chars().collect();
            chars[0] = chars[0].to_ascii_lowercase();
            s = chars.into_iter().collect();
        }

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_get_roundtrip() {
        let mut cache = FileStateCache::new();
        cache.record_read("/abs/foo.rs", None, "hello".into(), 1000, Some(0), Some(10));
        let st = cache.get("/abs/foo.rs", None).expect("recorded");
        assert_eq!(st.content, "hello");
        assert_eq!(st.timestamp, 1000);
        assert_eq!(st.offset, Some(0));
        assert_eq!(st.limit, Some(10));
        assert!(!st.is_full_read());
    }

    #[test]
    fn relative_and_absolute_resolve_to_same_key() {
        let cwd = PathBuf::from("/project");
        let k1 = FileStateCache::normalize_key("./foo.rs", Some(&cwd));
        let k2 = FileStateCache::normalize_key("/project/foo.rs", Some(&cwd));
        assert_eq!(k1, k2, "relative + absolute should collide");
    }

    #[test]
    fn relative_recorded_then_absolute_get() {
        let cwd = PathBuf::from("/project");
        let mut cache = FileStateCache::new();
        cache.record_read("./foo.rs", Some(&cwd), "x".into(), 1, None, None);
        // Edit usually passes the same relative or an absolute path.
        assert!(cache.get("/project/foo.rs", Some(&cwd)).is_some());
        assert!(cache.get("foo.rs", Some(&cwd)).is_some());
    }

    #[test]
    fn windows_backslashes_normalized() {
        let k1 = FileStateCache::normalize_key("C:\\Users\\test\\foo.rs", None);
        let k2 = FileStateCache::normalize_key("C:/Users/test/foo.rs", None);
        assert_eq!(k1, k2);
    }

    #[test]
    fn windows_drive_letter_case_insensitive() {
        let k1 = FileStateCache::normalize_key("C:/foo.rs", None);
        let k2 = FileStateCache::normalize_key("c:/foo.rs", None);
        assert_eq!(k1, k2);
    }

    #[test]
    fn record_write_clears_offset_limit() {
        let mut cache = FileStateCache::new();
        cache.record_read("/f.rs", None, "old".into(), 1, Some(5), Some(20));
        cache.record_write("/f.rs", None, "new".into(), 2);
        let st = cache.get("/f.rs", None).unwrap();
        assert_eq!(st.content, "new");
        assert_eq!(st.timestamp, 2);
        assert!(st.is_full_read(), "write should mark full view");
    }

    #[test]
    fn distinct_files_do_not_interfere() {
        let mut cache = FileStateCache::new();
        cache.record_read("/a.rs", None, "a".into(), 1, None, None);
        cache.record_read("/b.rs", None, "b".into(), 1, None, None);
        assert_eq!(cache.get("/a.rs", None).unwrap().content, "a");
        assert_eq!(cache.get("/b.rs", None).unwrap().content, "b");
    }

    #[test]
    fn missing_file_returns_none() {
        let cache = FileStateCache::new();
        assert!(cache.get("/nope.rs", None).is_none());
    }
}
