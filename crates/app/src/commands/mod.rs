//! Tauri command handlers, organized by domain.
//!
//! Each module exports `#[tauri::command]` async fns. Wiring lists them by
//! name in `main.rs::tauri::Builder::invoke_handler`.

pub mod balance;
pub mod brainstorm;
pub mod config;
pub mod fs;
pub mod hooks;
pub mod mcp;
pub mod messages;
pub mod models;
pub mod output_styles;
pub mod permissions;
pub mod projects;
pub mod pty;
pub mod questions;
pub mod rewind;
pub mod skill;
pub mod stats;
pub mod threads;

pub use balance::*;
pub use brainstorm::*;
pub use config::*;
pub use fs::*;
pub use hooks::*;
pub use mcp::*;
pub use messages::*;
pub use models::*;
pub use output_styles::*;
pub use permissions::*;
pub use projects::*;
pub use pty::*;
pub use questions::*;
pub use rewind::*;
pub use skill::*;
pub use stats::*;
pub use threads::*;
