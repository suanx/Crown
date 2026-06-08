//! Permission subsystem.
//!
//! Re-exports core types from `deepseek-tools::permission` so the Tool trait
//! and decision flow share the same type definitions without a circular dep.
//! The decision flow itself (and ToolPermissionContext) lives in this crate
//! and is filled in by tasks 3.2 / 3.4.

mod check;
mod context;
pub mod mode_cycle;
pub mod settings;

pub use check::{check_tool_permission, PermissionError};
pub use context::ToolPermissionContext;
pub use deepseek_tools::permission::*;
pub use mode_cycle::{check_accept_edits_shell, get_next_permission_mode};
pub use settings::{load_all_rules_from_disk, persist_permission_update, settings_path_for_source};
