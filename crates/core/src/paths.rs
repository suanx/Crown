//! Single source of truth for Crown's on-disk paths.
//!
//! All persisted files live under one **data root**. In the Tauri app the
//! root is `app.path().app_data_dir()` (= `%APPDATA%\crown` on Windows,
//! `~/Library/Application Support/crown` on macOS, `~/.local/share/crown` on
//! Linux) — see [`CrownPaths::with_root`]. Outside Tauri (tests / future CLI)
//! [`CrownPaths::from_dirs`] resolves `<data_dir>/crown` via the `dirs` crate.
//!
//! The app name is defined **once** here ([`APP_DIR_NAME`]); no other module
//! joins a literal app-directory name. This prevents the directory-name drift
//! that previously split data between `deepseek-agent` and
//! `com.deepseek-agent.dev`.

use std::path::{Path, PathBuf};

/// The single on-disk directory name for Crown's data root (used only by the
/// non-Tauri `dirs` fallback; the Tauri path uses the bundle identifier,
/// which is also `crown`).
pub const APP_DIR_NAME: &str = "crown";

/// Resolved set of Crown data paths, all derived from one `data_root`.
#[derive(Debug, Clone)]
pub struct CrownPaths {
    data_root: PathBuf,
}

impl CrownPaths {
    /// Construct from an explicit data root (the Tauri app passes
    /// `app.path().app_data_dir()` here).
    pub fn with_root(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    /// Fallback for non-Tauri contexts (tests, future CLI): `<data_dir>/crown`.
    /// Returns `Err` if the platform data dir cannot be resolved — callers
    /// must surface this, never silently fall back to the current directory.
    pub fn from_dirs() -> Result<Self, PathError> {
        let base = dirs::data_dir().ok_or(PathError::NoDataDir)?;
        Ok(Self {
            data_root: base.join(APP_DIR_NAME),
        })
    }

    /// The data root directory.
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// Create the data root directory if missing. Returns `Err` with the IO
    /// error if creation fails (e.g. permission denied) — callers surface it
    /// rather than degrading to the current directory.
    pub fn ensure_data_root(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_root)
    }

    /// `<root>/state.db`
    pub fn db_path(&self) -> PathBuf {
        self.data_root.join("state.db")
    }
    /// `<root>/mcp.json`
    pub fn mcp_config(&self) -> PathBuf {
        self.data_root.join("mcp.json")
    }
    /// `<root>/settings.json`
    pub fn settings(&self) -> PathBuf {
        self.data_root.join("settings.json")
    }
    /// `<root>/config.toml` (pricing override)
    pub fn pricing_override(&self) -> PathBuf {
        self.data_root.join("config.toml")
    }
    /// `<root>/skills`
    pub fn skills_dir(&self) -> PathBuf {
        self.data_root.join("skills")
    }
    /// `<root>/agents`
    pub fn agents_dir(&self) -> PathBuf {
        self.data_root.join("agents")
    }
    /// `<root>/commands`
    pub fn commands_dir(&self) -> PathBuf {
        self.data_root.join("commands")
    }
    /// `<root>/rules`
    pub fn rules_dir(&self) -> PathBuf {
        self.data_root.join("rules")
    }
    /// `<root>/output-styles`
    pub fn output_styles_dir(&self) -> PathBuf {
        self.data_root.join("output-styles")
    }
    /// `<root>/AGENTS.md` (global memory)
    pub fn global_memory(&self) -> PathBuf {
        self.data_root.join("AGENTS.md")
    }
}

/// Path resolution error — surfaced instead of degrading to `"."`.
#[derive(Debug, thiserror::Error)]
pub enum PathError {
    /// The platform data directory could not be resolved.
    #[error("could not resolve platform data directory")]
    NoDataDir,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn derives_all_paths_under_root() {
        let p = CrownPaths::with_root(PathBuf::from("/data/crown"));
        assert_eq!(p.db_path(), PathBuf::from("/data/crown/state.db"));
        assert_eq!(p.mcp_config(), PathBuf::from("/data/crown/mcp.json"));
        assert_eq!(p.settings(), PathBuf::from("/data/crown/settings.json"));
        assert_eq!(
            p.pricing_override(),
            PathBuf::from("/data/crown/config.toml")
        );
        assert_eq!(p.skills_dir(), PathBuf::from("/data/crown/skills"));
        assert_eq!(p.agents_dir(), PathBuf::from("/data/crown/agents"));
        assert_eq!(p.commands_dir(), PathBuf::from("/data/crown/commands"));
        assert_eq!(p.rules_dir(), PathBuf::from("/data/crown/rules"));
        assert_eq!(
            p.output_styles_dir(),
            PathBuf::from("/data/crown/output-styles")
        );
        assert_eq!(p.global_memory(), PathBuf::from("/data/crown/AGENTS.md"));
    }

    #[test]
    fn from_dirs_uses_crown_app_name() {
        let p = CrownPaths::from_dirs().expect("data dir resolves on test host");
        assert!(
            p.data_root().ends_with("crown"),
            "root = {:?}",
            p.data_root()
        );
    }

    #[test]
    fn app_dir_name_is_crown() {
        assert_eq!(APP_DIR_NAME, "crown");
    }

    #[test]
    fn ensure_dirs_creates_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("crown");
        let p = CrownPaths::with_root(root.clone());
        p.ensure_data_root().expect("create root");
        assert!(root.is_dir());
    }
}
