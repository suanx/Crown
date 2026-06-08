//! Built-in sub-agent types + provider-neutral model-tier resolution.

use crate::pricing::ProviderId;

/// Model price tier for a sub-agent. `Cheap` agents (read-only investigation,
/// planning) run on a cheaper model when the provider has one; `Standard`
/// agents use the parent thread's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    /// Use the provider's cheap model when available.
    Cheap,
    /// Use the parent thread's model.
    Standard,
}

/// A built-in sub-agent definition: name, prompt, allowed tools, tier, and
/// whether it is one-shot (no resumable id returned to the parent).
#[derive(Debug, Clone)]
pub struct AgentType {
    /// Stable identifier the model passes as `agent_type`.
    pub name: &'static str,
    /// Human-readable description (surfaced in the `task` tool spec).
    pub description: &'static str,
    /// System prompt injected for the sub-agent run.
    pub system_prompt: &'static str,
    /// Allowed tool names. Empty = all tools (except `task`).
    pub allowed_tools: &'static [&'static str],
    /// Model price tier.
    pub tier: ModelTier,
    /// One-shot agents return no resumable id (parent can't continue them).
    pub one_shot: bool,
}

/// General-purpose sub-agent: full tool access (minus `task`), multi-turn.
pub const GENERAL_PURPOSE: AgentType = AgentType {
    name: "general-purpose",
    description: "General agent for multi-step tasks with full tool access (read, write, shell, search). Resumable.",
    system_prompt: "You are a sub-agent spawned to complete a focused task on behalf of the main agent. \
You have full tool access except spawning further sub-agents. Work autonomously, verify your work, \
and end with a concise report of what you did and any important findings the main agent needs.",
    allowed_tools: &[],
    tier: ModelTier::Standard,
    one_shot: false,
};

/// Explore sub-agent: read-only investigation, returns a findings report.
pub const EXPLORE: AgentType = AgentType {
    name: "explore",
    description: "Read-only investigation of the codebase; returns a concise findings report. Use to understand code before acting. One-shot.",
    system_prompt: "You are an exploration sub-agent. Investigate the codebase using ONLY read-only tools \
(read_file, list_directory, grep, glob, web_search, web_fetch). Do NOT modify anything. \
Return a concise, well-structured report of your findings — file paths, key functions, how things connect.",
    allowed_tools: &[
        "read_file",
        "list_directory",
        "grep",
        "glob",
        "web_search",
        "web_fetch",
    ],
    tier: ModelTier::Cheap,
    one_shot: true,
};

/// Plan sub-agent: read-only, produces an implementation plan.
pub const PLAN: AgentType = AgentType {
    name: "plan",
    description: "Read-only planning; investigates then outputs a step-by-step implementation plan. One-shot.",
    system_prompt: "You are a planning sub-agent. Investigate the relevant code read-only, then output a \
clear, numbered, step-by-step implementation plan. Do NOT modify anything. Be specific about which \
files to touch and what to change.",
    allowed_tools: &[
        "read_file",
        "list_directory",
        "grep",
        "glob",
        "web_search",
        "web_fetch",
    ],
    tier: ModelTier::Cheap,
    one_shot: true,
};

/// All built-in agent types.
pub fn builtin_agents() -> &'static [AgentType] {
    // `static` so we can return a `'static` slice of owned `AgentType`s.
    static AGENTS: [AgentType; 3] = [GENERAL_PURPOSE, EXPLORE, PLAN];
    &AGENTS
}

/// Look up a built-in agent by name.
pub fn find_agent(name: &str) -> Option<&'static AgentType> {
    builtin_agents().iter().find(|a| a.name == name)
}

/// Resolve the model a sub-agent should run on.
///
/// ## Provider neutrality
///
/// Only DeepSeek has a hard-coded cheap model name (`deepseek-v4-flash`).
/// For every other provider — until a per-provider model-tier setting ships —
/// we fall back to the parent thread's model and never inject a DeepSeek
/// model name (see `.kiro/steering/provider-neutrality.md`). This mirrors
/// `AgentEngine::summary_model_for` used by context compaction.
pub fn subagent_model_for(provider: ProviderId, agent: &AgentType, parent_model: &str) -> String {
    match (agent.tier, provider) {
        (ModelTier::Cheap, ProviderId::Deepseek) => "deepseek-v4-flash".to_string(),
        _ => parent_model.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_is_readonly_cheap_oneshot() {
        let a = find_agent("explore").expect("explore exists");
        assert!(a.one_shot);
        assert_eq!(a.tier, ModelTier::Cheap);
        assert!(a.allowed_tools.contains(&"read_file"));
        assert!(!a.allowed_tools.contains(&"write_file"));
        assert!(!a.allowed_tools.contains(&"task"));
    }

    #[test]
    fn general_purpose_is_standard_multiturn_alltools() {
        let a = find_agent("general-purpose").expect("exists");
        assert!(!a.one_shot);
        assert_eq!(a.tier, ModelTier::Standard);
        assert!(a.allowed_tools.is_empty(), "empty = all tools");
    }

    #[test]
    fn unknown_agent_is_none() {
        assert!(find_agent("nope").is_none());
    }

    #[test]
    fn deepseek_cheap_uses_flash() {
        let a = find_agent("explore").unwrap();
        assert_eq!(
            subagent_model_for(ProviderId::Deepseek, a, "deepseek-v4-pro"),
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn non_deepseek_cheap_keeps_parent_model() {
        // Provider neutrality: never inject a DeepSeek model name for others.
        let a = find_agent("explore").unwrap();
        let other = ProviderId::from_str_lossy("openai");
        assert_eq!(subagent_model_for(other, a, "gpt-x"), "gpt-x");
    }

    #[test]
    fn standard_tier_always_parent_model() {
        let a = find_agent("general-purpose").unwrap();
        assert_eq!(
            subagent_model_for(ProviderId::Deepseek, a, "deepseek-v4-pro"),
            "deepseek-v4-pro"
        );
    }
}
