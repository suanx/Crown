//! SQLite-backed database handle.
//!
//! Owns a single [`rusqlite::Connection`] guarded by a [`parking_lot::Mutex`].
//! WAL mode allows concurrent reads from other connections, but for the
//! simplicity P4 demands we keep one connection here — all reads/writes
//! serialize through the mutex but stay fast (single-digit ms for typical ops).

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::Connection;
use thiserror::Error;

use crate::schema;

/// Database error.
#[derive(Debug, Error)]
pub enum DbError {
    /// Underlying rusqlite error.
    #[error("rusqlite: {0}")]
    Rusqlite(#[from] rusqlite::Error),
    /// IO error (path / fs).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Path resolution failure.
    #[error("path: {0}")]
    Path(String),
}

/// SQLite-backed database handle.
///
/// Safe to clone the wrapping [`std::sync::Arc`]; internal state is mutex
/// guarded.
pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// Open or create the database at `path`. Creates parent directories if
    /// missing, applies WAL pragmas, and runs schema migrations idempotently.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(&path)?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    /// Default app database path: `<data_dir>/crown/state.db`.
    ///
    /// This is a **fallback** for non-Tauri contexts; the Tauri app resolves
    /// the path via `CrownPaths` (rooted at `app_data_dir()`) instead. Both
    /// resolve to the same place on all platforms:
    /// - Windows: `%APPDATA%\crown\state.db`
    /// - macOS:   `~/Library/Application Support/crown/state.db`
    /// - Linux:   `~/.local/share/crown/state.db`
    pub fn default_path() -> Result<PathBuf, DbError> {
        let base = dirs::data_dir().ok_or_else(|| DbError::Path("no data dir".into()))?;
        Ok(base.join("crown").join("state.db"))
    }

    /// Acquire the connection lock. Used internally by repo modules.
    pub fn conn(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Path the database was opened from (for diagnostics).
    pub fn path(&self) -> &Path {
        &self.path
    }
}
