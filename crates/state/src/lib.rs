//! State persistence layer: SQLite-backed thread / messages / checkpoints / usage storage.

#![deny(unsafe_code)]
#![deny(clippy::unwrap_used)]
#![warn(missing_docs)]

mod db;
mod repo;
mod schema;

pub use db::{Database, DbError};
pub use repo::{
    checkpoints::{CheckpointInsert, CheckpointRepo, CheckpointRow},
    file_history::{FileHistoryInsert, FileHistoryRepo, FileHistoryRow},
    messages::{MessageInsert, MessageRepo, MessageRow},
    projects::{Project, ProjectInsert, ProjectRepo, ProjectSummary, ProjectUpdate},
    threads::{Thread, ThreadInsert, ThreadRepo, ThreadSummary, ThreadUpdate},
    tool_trace::{ToolDispatchTraceInsert, ToolDispatchTraceRepo, ToolDispatchTraceRow},
    usage::{CacheReadBreakdownRow, UsageAggregate, UsageInsert, UsageRepo},
};
