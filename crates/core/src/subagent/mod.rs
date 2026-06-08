//! Sub-agent subsystem (P4): built-in agent type definitions + the
//! provider-neutral model-tier resolver. The actual runner that drives a
//! sub-agent's engine loop lives in the app layer (it needs the Tauri event
//! sink); this module is pure data + logic so it stays testable.

pub mod types;

pub use types::{
    builtin_agents, find_agent, subagent_model_for, AgentType, ModelTier, GENERAL_PURPOSE,
};
