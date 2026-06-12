//! ThreadState — per-thread runtime state held in memory.
//! ThreadCache — LRU-bounded cache mapping thread_id → Arc<ThreadState>.
//!
//! Held in `Arc` so the active thread pointer and the cache share the same
//! allocation (no clones on switch). LRU=3 by default keeps memory bounded
//! at ~3 ThreadStates regardless of total persisted threads.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use lru::LruCache;
use parking_lot::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use ulid::Ulid;

use crate::context::{AppendOnlyLog, ImmutablePrefix};
use crate::permission::ToolPermissionContext;
use crate::pricing::ProviderId;
use deepseek_tools::file_state::FileStateCache;

/// Track which files changed during a single turn, for diff summaries and
/// post-hoc auditing. Codex-aligned: `codex-rs/core/src/turn_diff_tracker.rs`.
#[derive(Debug, Default, Clone)]
pub struct TurnDiffTracker {
    /// Files created this turn (absolute paths).
    pub created: Vec<String>,
    /// Files modified this turn (absolute paths).
    pub modified: Vec<String>,
    /// Files deleted this turn (absolute paths).
    pub deleted: Vec<String>,
}

impl TurnDiffTracker {
    /// Returns true if any files changed this turn.
    pub fn has_changes(&self) -> bool {
        !self.created.is_empty() || !self.modified.is_empty() || !self.deleted.is_empty()
    }

    /// Total number of files touched this turn.
    pub fn total_changed(&self) -> usize {
        self.created.len() + self.modified.len() + self.deleted.len()
    }

    pub fn clear(&mut self) {
        self.created.clear();
        self.modified.clear();
        self.deleted.clear();
    }
}

/// Thread identifier (ULID string).
pub type ThreadId = String;

/// Per-thread runtime state. Held in `Arc` so cache and active pointer share
/// memory. Internal mutability via `RwLock` for log + permission_ctx.
pub struct ThreadState {
    /// Thread ID (ULID).
    pub id: ThreadId,
    /// Model identifier. Mutable so `switch_model` can update it.
    pub model: RwLock<String>,
    /// 每个线程独立的推理强度，可由输入区模型菜单更新。
    pub thinking_effort: RwLock<String>,
    /// LLM provider for this thread. Drives cost computation, context-window
    /// lookup, and — critically — provider-gated optimizations (compaction
    /// thinking toggle, etc.) per `.kiro/steering/provider-neutrality.md`.
    pub provider: RwLock<ProviderId>,
    /// 供应商原始 ID。`ProviderId::Other` 只能表达“未知供应商”，这里保留
    /// 真实字符串用于运行时解析用户配置。
    /// 供应商原始 ID。`ProviderId::Other` 只能表达"未知供应商"，这里保留
    /// 真实字符串用于运行时解析用户配置。
    pub provider_id: RwLock<String>,
    /// Working directory for tool calls (informational; tools resolve paths
    /// independently against process cwd).
    pub cwd: Option<PathBuf>,
    /// 用户自定义上下文长度覆盖（0 = 使用定价表默认值）。
    pub context_window_override: RwLock<Option<usize>>,
    /// Immutable system prefix.
    pub prefix: ImmutablePrefix,
    /// Conversation log (append-only).
    pub log: RwLock<AppendOnlyLog>,
    /// Permission context (mode + rules).
    pub permission_ctx: RwLock<ToolPermissionContext>,
    /// Current turn's abort token. Replaced at the start of each turn.
    pub abort_token: ArcSwapOption<CancellationToken>,
    /// Read-before-write file state cache. Survives across turns (session
    /// scoped) so a file read in one turn can be edited in the next.
    pub file_state: Arc<Mutex<FileStateCache>>,
    /// Session todo list (TodoWrite tool). Shared with ToolContext each turn.
    pub todos: deepseek_tools::todo::TodoList,
    /// Per-turn file-change tracker (Codex-aligned). Cleared at the start
    /// of each turn; populated by write_file / edit_file tool executions.
    pub turn_diff: Arc<Mutex<TurnDiffTracker>>,
    /// Creation time (unix ms).
    pub created_at: i64,
    /// Last activity (unix ms). Updated on each message.
    pub updated_at: AtomicI64,
}

impl ThreadState {
    /// Construct from scratch.
    pub fn new(
        id: ThreadId,
        model: String,
        thinking_effort: String,
        provider: ProviderId,
        provider_id: String,
        cwd: Option<PathBuf>,
        permission_ctx: ToolPermissionContext,
        system_prompt: String,
        context_window_override: Option<usize>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            model: RwLock::new(model),
            thinking_effort: RwLock::new(thinking_effort),
            provider: RwLock::new(provider),
            provider_id: RwLock::new(provider_id),
            cwd,
            context_window_override: RwLock::new(context_window_override),
            prefix: ImmutablePrefix::new(system_prompt),
            log: RwLock::new(AppendOnlyLog::default()),
            permission_ctx: RwLock::new(permission_ctx),
            abort_token: ArcSwapOption::empty(),
            file_state: Arc::new(Mutex::new(FileStateCache::new())),
            todos: Arc::new(Mutex::new(Vec::new())),
            turn_diff: Arc::new(Mutex::new(TurnDiffTracker::default())),
            created_at: now,
            updated_at: AtomicI64::new(now),
        }
    }

    /// Generate a new ULID-based ID.
    pub fn new_id() -> ThreadId {
        Ulid::new().to_string()
    }

    /// Bump updated_at to now.
    pub fn touch(&self) {
        self.updated_at
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Release);
    }

    /// Read updated_at.
    pub fn updated_at(&self) -> i64 {
        self.updated_at.load(Ordering::Acquire)
    }
}

/// LRU-bounded thread cache. Capacity = 3 by default.
///
/// Insert `Arc<ThreadState>` for fast multi-reference. The active thread
/// pointer (held outside) shares the same Arc, so promotions are cheap.
pub struct ThreadCache {
    inner: Mutex<LruCache<ThreadId, Arc<ThreadState>>>,
}

impl ThreadCache {
    /// New cache with `capacity` slots. Panics if capacity is 0.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("capacity must be > 0"),
            )),
        }
    }

    /// Default capacity: 3.
    pub fn with_default_capacity() -> Self {
        Self::new(3)
    }

    /// Get and promote (move-to-front).
    pub fn get(&self, id: &str) -> Option<Arc<ThreadState>> {
        self.inner.lock().get(id).cloned()
    }

    /// Insert; evicts LRU if at capacity.
    /// Returns the evicted state (if any) for observability.
    pub fn put(&self, state: Arc<ThreadState>) -> Option<Arc<ThreadState>> {
        let id = state.id.clone();
        self.inner.lock().push(id, state).map(|(_, v)| v)
    }

    /// Remove (e.g. on delete_thread).
    pub fn remove(&self, id: &str) -> Option<Arc<ThreadState>> {
        self.inner.lock().pop(id)
    }

    /// Evict all cached thread states. Used when a global prompt input
    /// changes (e.g. the active output-style) so each thread recomposes its
    /// system prompt on next load.
    pub fn clear(&self) {
        self.inner.lock().clear();
    }

    /// Approximate size (cheap; just a snapshot).
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::ToolPermissionContext;

    fn mk_thread(id: &str) -> Arc<ThreadState> {
        Arc::new(ThreadState::new(
            id.into(),
            "deepseek-v4-flash".into(),
            "medium".into(),
            ProviderId::Deepseek,
            None,
            ToolPermissionContext::default(),
            "system".into(),
            None,
        ))
    }

    #[test]
    fn put_and_get_promotes() {
        let cache = ThreadCache::new(2);
        cache.put(mk_thread("a"));
        cache.put(mk_thread("b"));
        // Touch a → a is most recently used
        let _ = cache.get("a");
        // Insert c → should evict b (LRU)
        cache.put(mk_thread("c"));
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn remove_evicts() {
        let cache = ThreadCache::new(3);
        cache.put(mk_thread("a"));
        let removed = cache.remove("a");
        assert!(removed.is_some());
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn arc_sharing_does_not_clone_state() {
        let cache = ThreadCache::new(3);
        let s1 = mk_thread("x");
        let original_strong = Arc::strong_count(&s1);
        cache.put(Arc::clone(&s1));
        let s2 = cache.get("x").unwrap();
        // Cache holds 1, we hold 2 (s1, s2)
        assert!(Arc::strong_count(&s2) > original_strong);
        // Same allocation — pointer-equal
        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn touch_updates_timestamp() {
        let s = mk_thread("a");
        let before = s.updated_at();
        std::thread::sleep(std::time::Duration::from_millis(2));
        s.touch();
        let after = s.updated_at();
        assert!(after > before);
    }

    #[test]
    fn new_id_is_unique() {
        let a = ThreadState::new_id();
        let b = ThreadState::new_id();
        assert_ne!(a, b);
    }
}
