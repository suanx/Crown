//! OpenAI-compatible tool specs and default tool registration.
//!
//! [`build_tool_specs`] produces the JSON schemas the model needs to know
//! which tools exist and how to call them. [`register_default_tools`]
//! installs all built-in tools onto a [`crate::ToolRegistry`].
//!
//! The spec list and the registry registration must stay in lockstep —
//! every tool advertised to the model must be dispatchable, and every
//! registered tool must be advertised. The integration test
//! [`tests::test_specs_match_registered_tools`] enforces that invariant.

use std::sync::Arc;

use deepseek_client::types::{FunctionSpec, ToolSpec};
use serde_json::json;

use crate::filesystem::{EditFileTool, ListDirectoryTool, ReadFileTool, WriteFileTool};
use crate::glob_tool::GlobTool;
use crate::grep::GrepTool;
use crate::shell::RunCommandTool;
use crate::skill_tool::SkillTool;
use crate::todo::TodoWriteTool;
use crate::web::{WebFetchTool, WebSearchTool, WebToolsState};
use crate::Tool;
use crate::ToolRegistry;

/// Build the spec list from a live registry: built-in specs (static) plus
/// any dynamically-registered tools (e.g. MCP proxies) that supply their own
/// `spec()`. This is what the engine sends to the model so MCP tools are
/// advertised alongside built-ins.
/// Build the tool specs advertised to the model **for a specific registry**.
///
/// Only emits a spec for a tool that is actually present in `registry`. This
/// matters for sub-agents: they run on a restricted registry (e.g. `explore`
/// is read-only, no `run_command`), and the model must NOT be told about
/// tools it cannot call — otherwise it tries them and gets an "unavailable"
/// error (the tool was listed but absent). Built-ins keep their curated
/// static specs; dynamically-registered tools (MCP proxies) contribute their
/// own `spec()`.
pub fn build_tool_specs_from_registry(registry: &ToolRegistry) -> Vec<ToolSpec> {
    let present: std::collections::HashSet<String> = registry
        .all_tools()
        .iter()
        .map(|t| t.name().to_string())
        .collect();

    // Keep only built-in specs whose tool is actually registered.
    let mut specs: Vec<ToolSpec> = build_tool_specs()
        .into_iter()
        .filter(|s| present.contains(&s.function.name))
        .collect();

    // Append non-built-in (e.g. MCP) tools that supply their own spec.
    let builtin: std::collections::HashSet<String> = build_tool_specs()
        .iter()
        .map(|s| s.function.name.clone())
        .collect();
    for tool in registry.all_tools() {
        if builtin.contains(tool.name()) {
            continue;
        }
        if let Some(spec) = tool.spec() {
            specs.push(spec);
        }
    }
    specs
}

/// Build the OpenAI-compatible tool spec list advertised to the model.
///
/// Returns one [`ToolSpec`] per built-in tool, in a stable order. The
/// `parameters` field on each spec is a JSON Schema object describing the
/// arguments the tool accepts; descriptions intentionally call out caps and
/// defaults so the model can reason about safety boundaries before issuing
/// a call.
pub fn build_tool_specs() -> Vec<ToolSpec> {
    vec![
        // ── read_file ────────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "read_file".to_string(),
                description: "Reads a file from the local filesystem. You can access any file directly by using this tool.\n\nUsage:\n- The path parameter can be absolute or relative to the working directory.\n- By default, reads up to 2000 lines starting from the beginning of the file.\n- You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters.\n- Results are returned using cat -n format, with line numbers starting at 1.\n- This tool can only read files, not directories. To read a directory, use list_directory.\n- If you read a file that exists but has empty contents you will receive a warning.\n- IMPORTANT: You MUST read a file before editing it. edit_file will error if you haven't read the file first."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path (absolute or relative to cwd)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "0-indexed starting line (default 0)",
                            "minimum": 0
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Number of lines to read; required for files >1MB",
                            "minimum": 1
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        // ── list_directory ──────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "list_directory".to_string(),
                description: "List files and directories at a path. Skips noise dirs \
                              (.git, node_modules, target, dist, build, venv). \
                              Capped at 500 entries."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list"
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "Descend into subdirectories (default false)"
                        },
                        "max_depth": {
                            "type": "integer",
                            "description": "Maximum recursion depth when recursive=true (default 3)",
                            "minimum": 1,
                            "maximum": 6
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        // ── write_file ──────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "write_file".to_string(),
                description: "Write content to a file, creating it if it doesn't exist. Creates parent directories automatically.\n\nUsage:\n- Use this for creating NEW files only. For modifying existing files, ALWAYS use edit_file instead — it provides better visibility for the user to review changes.\n- If the file already exists, it will be completely overwritten. Only overwrite if you intend to replace the entire file.\n- ALWAYS prefer edit_file for modifications to existing files. write_file is for wholesale creation/overwrite only.\n- For existing files you have already read, use edit_file with the exact text to change."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Destination file path"
                        },
                        "content": {
                            "type": "string",
                            "description": "UTF-8 file contents to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        // ── edit_file ───────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "edit_file".to_string(),
                description: "Performs exact string replacements in files.\n\nUsage:\n- You MUST use read_file at least once before editing. This tool will error if you attempt an edit without reading the file first.\n- When editing text from read_file output, preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. Never include any part of the line number prefix in old_string or new_string.\n- ALWAYS prefer editing existing files. NEVER write new files unless explicitly required.\n- The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance.\n- Use the smallest old_string that's clearly unique — usually 2-4 adjacent lines is sufficient. Avoid including 10+ lines of context when less uniquely identifies the target.\n- Use replace_all for renaming variables or replacing a string across the entire file.\n- Create a new file by setting old_string to empty string — the new_string becomes the entire file content."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "Exact text to find (empty string = create new file)"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "Replacement text"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "Replace every occurrence instead of erroring on ambiguity (default false)"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
        },
        // ── run_command ─────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "run_command".to_string(),
                description: "Executes a shell command and returns its output.\n\nThe working directory is the project root. Shell: PowerShell on Windows, zsh on macOS, bash on Linux.\n\nIMPORTANT: Avoid using this tool to run cat, head, tail, sed, awk, find, or grep commands. Instead use the dedicated tools (read_file, edit_file, glob, grep) as they provide better UX.\n\nInstructions:\n- Always quote file paths that contain spaces.\n- You may specify a timeout (up to 600s). Default is 120s.\n- stdin is CLOSED. Commands that prompt for user input will hang. ALWAYS use non-interactive flags: git commit --no-edit, npm install --yes, apt-get -y, pip install --no-input.\n- NEVER use interactive flags like git rebase -i or git add -i.\n- When issuing multiple independent commands, make multiple run_command calls in parallel. For sequential dependencies, chain with && in a single call.\n- For git commands: prefer creating NEW commits over amending. Never skip hooks (--no-verify) unless explicitly asked. Never force-push to main/master.\n- NEVER run destructive git commands (push --force, reset --hard, checkout ., clean -f) unless the user explicitly requests them.\n- If a command times out, it likely hung waiting for input. Tell the user and use a non-interactive alternative."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Command line forwarded to the platform shell"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Working directory for the spawned process"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Per-call timeout in seconds (default 120, max 600)",
                            "minimum": 1,
                            "maximum": 600
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        // ── web_search ──────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "web_search".to_string(),
                description: "Search the web for current information. Returns titles, URLs, \
                              and snippets. Use for up-to-date info, documentation, \
                              library versions, or anything beyond training data."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (max 500 characters)"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Number of results to return (default 5, max 10)",
                            "minimum": 1,
                            "maximum": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        // ── web_fetch ───────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "web_fetch".to_string(),
                description: "Fetch and extract content from a URL. Returns the page's \
                              main text content (HTML is converted to plain text, scripts \
                              and navigation stripped). Cached for 15 minutes. \
                              Max output: 80K chars."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to fetch (must be https or http, no private IPs)"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "Optional: what to look for in the page content"
                        }
                    },
                    "required": ["url"]
                }),
            },
        },
        // ── grep ────────────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "grep".to_string(),
                description: "Search file contents using regular expressions (fast, ripgrep-grade).\n\nUsage:\n- Use this for searching CONTENT inside files. For searching file NAMES, use glob instead.\n- Supports full regex syntax.\n- output_mode controls what's returned:\n  - 'files_with_matches' (default): just file paths that contain matches\n  - 'content': matching lines with path:line:text format (like grep -n)\n  - 'count': per-file match counts\n- Respects .gitignore. Skips binary files.\n- Use the glob parameter to filter which files to search (e.g. '*.ts' to only search TypeScript).\n- For simple text searches, this is faster and more reliable than running shell grep."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regular expression to search for in file contents"
                        },
                        "path": {
                            "type": "string",
                            "description": "Root directory to search (default cwd or '.')"
                        },
                        "glob": {
                            "type": "string",
                            "description": "Optional glob filter on file paths (e.g. '*.rs', '**/*.ts')"
                        },
                        "output_mode": {
                            "type": "string",
                            "enum": ["content", "files_with_matches", "count"],
                            "description": "Output format: 'content', 'files_with_matches' (default), or 'count'"
                        },
                        "context": {
                            "type": "integer",
                            "description": "Lines of context around each match (content mode only)",
                            "minimum": 0
                        },
                        "case_insensitive": {
                            "type": "boolean",
                            "description": "Case-insensitive search (default false)"
                        },
                        "head_limit": {
                            "type": "integer",
                            "description": "Maximum results to return (default 250)",
                            "minimum": 1
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        // ── glob ────────────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "glob".to_string(),
                description: "Find files by glob pattern. Use this instead of find or ls.\n\nUsage:\n- Use this for searching FILE NAMES/PATHS. For searching file CONTENTS, use grep instead.\n- Returns paths sorted by modification time (newest first) — useful for finding recently changed files.\n- Respects .gitignore. Skips hidden directories by default.\n- Common patterns: '**/*.rs' (all Rust files), 'src/**/*.ts' (TypeScript in src), '*.json' (JSON in root).\n- Use for 'what files exist', 'what changed recently', 'all files of type X'."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern, e.g. '**/*.rs' or 'src/**/*.ts'"
                        },
                        "path": {
                            "type": "string",
                            "description": "Root directory to search (default cwd or '.')"
                        },
                        "head_limit": {
                            "type": "integer",
                            "description": "Maximum results to return (default 250)",
                            "minimum": 1
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        // ── todo_write ──────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "todo_write".to_string(),
                description: "Update the todo list for the current session. Use proactively and often to track progress and pending tasks.\n\nWhen to use:\n- Complex multi-step tasks (3+ distinct steps)\n- User provides multiple tasks (numbered or comma-separated)\n- After receiving new instructions — immediately capture them\n- When starting work on a task — mark it in_progress BEFORE beginning\n- After completing a task — mark it completed immediately\n\nWhen NOT to use:\n- Single straightforward tasks (just do them directly)\n- Trivial tasks that can be done in < 3 steps\n- Purely conversational/informational questions\n\nRules:\n- Make sure at least one task is in_progress at all times during work\n- Mark tasks completed IMMEDIATELY after finishing (don't batch)\n- ONLY mark completed when FULLY done (tests pass, no partial work)\n- Remove tasks that are no longer relevant\n- Always provide both 'content' (imperative: 'Run tests') and 'activeForm' (present continuous: 'Running tests')\n\nExamples — WHEN TO USE:\n<example>\nUser: Add a dark mode toggle to settings, and run the tests when done.\nAssistant: *creates todos: 1) Add toggle component 2) Wire theme state 3) Apply dark styles 4) Run tests & fix failures* then starts task 1.\nWhy: multi-step feature spanning UI + state + styling, and the user explicitly asked to run tests — each step is tracked so none is missed.\n</example>\n<example>\nUser: Rename getCwd to getCurrentWorkingDirectory across the project.\nAssistant: *greps for getCwd, finds 15 uses in 8 files, creates one todo per file* then works through them.\nWhy: the search revealed a multi-file change; the list ensures every occurrence is updated consistently.\n</example>\n\nExamples — WHEN NOT TO USE:\n<example>\nUser: How do I print 'Hello World' in Python?\nAssistant: answers directly with print(\"Hello World\"). No todo list.\nWhy: a single trivial informational request — tracking adds no value.\n</example>\n<example>\nUser: Add a comment to the calculateTotal function.\nAssistant: edits the file directly. No todo list.\nWhy: one straightforward change confined to a single location.\n</example>"
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "todos": {
                            "type": "array",
                            "description": "The complete updated todo list (replaces the previous list entirely)",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "content": {"type": "string", "description": "Imperative task description"},
                                    "activeForm": {"type": "string", "description": "Present-continuous form shown while active"},
                                    "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                                },
                                "required": ["content", "activeForm", "status"]
                            }
                        }
                    },
                    "required": ["todos"]
                }),
            },
        },
        // ── skill ─────────────────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "skill".to_string(),
                description: "Execute a skill — a reusable, named instruction pack. Available skills are listed in a system-reminder with their names and descriptions. When the user's request matches a skill's purpose, call this tool with the skill name BEFORE doing the task yourself; the skill's full instructions will be loaded and you must then follow them. Do not invoke a skill that is already running.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "skill": {
                            "type": "string",
                            "description": "The skill name (e.g. 'commit', 'review-pr')"
                        },
                        "args": {
                            "type": "string",
                            "description": "Optional arguments passed to the skill (substituted for $ARGUMENTS)"
                        }
                    },
                    "required": ["skill"]
                }),
            },
        },
        // ── task (sub-agent) ────────────────────────────────────────────
        ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: "task".to_string(),
                description: "Delegate a focused task to a sub-agent with its own isolated context. Use for large investigations, parallelizable work, or to keep your own context clean. agent_type: 'general-purpose' (full tools, resumable), 'explore' (read-only investigation, one-shot), 'plan' (read-only, produces an implementation plan, one-shot). The sub-agent runs autonomously and returns a report. For 'general-purpose', the result includes a subagent_id you can pass back to continue it.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "Short (3-5 word) description of the task" },
                        "prompt": { "type": "string", "description": "The full task for the sub-agent to perform" },
                        "agent_type": {
                            "type": "string",
                            "enum": ["general-purpose", "explore", "plan"],
                            "description": "Which sub-agent to use (default general-purpose)"
                        },
                        "subagent_id": { "type": "string", "description": "Resume an existing sub-agent (from a prior task result)" }
                    },
                    "required": ["prompt"]
                }),
            },
        },
        // ── ask_user_question ─────────────────────────────────────────────
        // Reuse the tool's own canonical spec to avoid duplicating the schema.
        crate::ask_user_question::AskUserQuestionTool
            .spec()
            .expect("AskUserQuestionTool always returns a spec"),
    ]
}

/// Register all built-in tools onto `registry`.
///
/// Order does not matter for correctness — the registry indexes tools by
/// name — but it is kept consistent with [`build_tool_specs`] so diagnostic
/// output stays readable. Read-only tools are listed first, mutating tools
/// last.
pub fn register_default_tools(registry: &ToolRegistry, web_state: Arc<WebToolsState>) {
    registry.register(Arc::new(ReadFileTool));
    registry.register(Arc::new(ListDirectoryTool));
    registry.register(Arc::new(WriteFileTool));
    registry.register(Arc::new(EditFileTool));
    registry.register(Arc::new(RunCommandTool));
    registry.register(Arc::new(WebSearchTool::new(web_state.clone())));
    registry.register(Arc::new(WebFetchTool::new(web_state)));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(TodoWriteTool));
    registry.register(Arc::new(SkillTool));
    registry.register(Arc::new(crate::task_tool::TaskTool));
    registry.register(Arc::new(crate::ask_user_question::AskUserQuestionTool));
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// All built-in tools must be advertised.
    #[test]
    fn test_build_tool_specs_returns_all_builtins() {
        let specs = build_tool_specs();
        assert_eq!(
            specs.len(),
            13,
            "expected 13 tool specs, got {}",
            specs.len()
        );
    }

    /// The DeepSeek tool-call protocol only supports `function`-typed tools
    /// today; guard against anyone introducing a different `tool_type`.
    #[test]
    fn test_all_specs_have_function_type() {
        for spec in build_tool_specs() {
            assert_eq!(
                spec.tool_type, "function",
                "tool {:?} has unexpected type {:?}",
                spec.function.name, spec.tool_type
            );
        }
    }

    /// Tool names are the model-facing identifier; collisions would make
    /// dispatch ambiguous.
    #[test]
    fn test_all_specs_have_unique_names() {
        let names: HashSet<String> = build_tool_specs()
            .into_iter()
            .map(|s| s.function.name)
            .collect();
        assert_eq!(names.len(), 13, "duplicate tool name detected: {:?}", names);
    }

    /// Every advertised spec must correspond to a registered tool, and vice
    /// versa, so the model can never call a tool the runtime doesn't know
    /// how to dispatch.
    #[test]
    fn test_specs_match_registered_tools() {
        let registry = ToolRegistry::new();
        let web_state = Arc::new(WebToolsState::default());
        register_default_tools(&registry, web_state);

        let mut spec_names: Vec<String> = build_tool_specs()
            .into_iter()
            .map(|s| s.function.name)
            .collect();
        spec_names.sort();

        let registry_names = registry.list_names();

        assert_eq!(
            spec_names, registry_names,
            "spec names and registry names diverged"
        );
    }

    /// Sanity check that the registration helper installs all built-in tools.
    #[test]
    fn test_register_default_tools_count() {
        let registry = ToolRegistry::new();
        let web_state = Arc::new(WebToolsState::default());
        register_default_tools(&registry, web_state);
        assert_eq!(registry.len(), 13);
    }

    #[test]
    fn registers_ask_user_question() {
        let registry = ToolRegistry::new();
        let web_state = Arc::new(WebToolsState::default());
        register_default_tools(&registry, web_state);
        assert!(registry.get("ask_user_question").is_some());
    }

    /// A restricted registry (e.g. a read-only sub-agent) must advertise ONLY
    /// the tools it actually has — the model must not see `run_command` if the
    /// sub-agent can't run it (otherwise it tries and gets "unavailable").
    #[test]
    fn restricted_registry_advertises_only_present_tools() {
        let registry = ToolRegistry::new();
        let web_state = Arc::new(WebToolsState::default());
        register_default_tools(&registry, web_state);

        // Mirror an `explore` sub-agent: read-only allowlist, exclude task.
        let restricted = registry.subset(
            &[
                "read_file",
                "list_directory",
                "grep",
                "glob",
                "web_search",
                "web_fetch",
            ],
            &["task"],
        );

        let advertised: HashSet<String> = build_tool_specs_from_registry(&restricted)
            .into_iter()
            .map(|s| s.function.name)
            .collect();

        assert!(advertised.contains("read_file"));
        assert!(advertised.contains("grep"));
        assert!(
            !advertised.contains("run_command"),
            "restricted read-only registry must NOT advertise run_command"
        );
        assert!(
            !advertised.contains("write_file"),
            "restricted read-only registry must NOT advertise write_file"
        );
        // Exactly the 6 allowed tools, nothing more.
        assert_eq!(advertised.len(), 6, "got: {advertised:?}");
    }
}
