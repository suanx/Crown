//! `ask_user_question` 工具 + QuestionGate 阻塞协议。
//!
//! 面对模糊需求时，agent 一次提交一组结构化多选题；用户在前端上浮面板
//! 逐题作答，答案结构化回灌。阻塞骨架照抄 PermissionGate（见
//! `crates/core/src/gate.rs` + `crates/app/src/gate_impl.rs`），但独立命名、
//! 独立事件，语义隔离。
//!
//! ## 方向约束
//! - [`QuestionRequest`] emit-only（仅 `Serialize`）：后端→前端。
//! - [`QuestionAnswers`] receive-only（仅 `Deserialize`）：前端→后端。
//!
//! ## 供应商中立
//! 全程不碰任何供应商专属字段，对所有 provider 行为一致。

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use deepseek_client::types::{FunctionSpec, ToolSpec};

use crate::types::ToolError;
use crate::{Tool, ToolContext};

/// 一个待问的选项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionOption {
    /// 展示文本（1-5 词）。
    pub label: String,
    /// 选项含义/后果说明。
    pub description: String,
}

/// 一道题。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// 极短标签（chip），翻页指示 + 面板顶部用。
    pub header: String,
    /// 完整问题，以问号结尾。
    pub question: String,
    /// 单选(false)/多选(true)。
    #[serde(default)]
    pub multi_select: bool,
    /// 2-4 个互斥选项（多选时可不互斥）。
    pub options: Vec<QuestionOption>,
}

/// 推给前端的问题请求（**emit-only**）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequest {
    /// 所属线程。
    pub thread_id: String,
    /// 触发本次提问的 `tool_use` 块 id。
    pub tool_use_id: String,
    /// 待问的一组题（1-4）。
    pub questions: Vec<Question>,
}

/// 单题答案。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AnswerItem {
    /// 对应的题干文本。
    pub question: String,
    /// 选中的 label（单选 1 个，多选 N 个）。
    #[serde(default)]
    pub selected: Vec<String>,
    /// "其他"自由填空（折叠链接展开后填的），无则 `None`。
    #[serde(default)]
    pub other: Option<String>,
}

/// 用户提交的答案（**receive-only**）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswers {
    /// 对应的 `tool_use` id。
    pub tool_use_id: String,
    /// 用户点了"取消"（= 拒绝工具调用）。
    #[serde(default)]
    pub cancelled: bool,
    /// 逐题答案。
    #[serde(default)]
    pub answers: Vec<AnswerItem>,
}

/// 阻塞结果。
#[derive(Debug, Clone)]
pub enum QuestionOutcome {
    /// 用户作答完成。
    Answered(Vec<AnswerItem>),
    /// 用户取消澄清。
    Cancelled,
}

/// QuestionGate 错误。
#[derive(Debug, Error)]
pub enum QuestionGateError {
    /// 前端通道关闭（窗口关闭等）。
    #[error("question channel closed")]
    Closed,
    /// emit 事件失败。
    #[error("emit failed: {0}")]
    Emit(String),
    /// 当前 turn 在用户应答前被中止。
    #[error("aborted")]
    Aborted,
}

/// agent 发问时阻塞等待前端答复的协议。
///
/// 实现：
/// - `TauriQuestionGate`（生产，`crates/app/src/question_gate_impl.rs`）：
///   emit `question:request` 并等待匹配的 `submit_answers` invoke。
/// - 测试用 mock gate：直接返回预设结果，无 IPC。
#[async_trait]
pub trait QuestionGate: Send + Sync {
    /// 显示问答 UI 并等待用户提交。`abort` 触发时丢弃请求返回 `Aborted`。
    async fn ask(
        &self,
        req: QuestionRequest,
        abort: CancellationToken,
    ) -> Result<QuestionOutcome, QuestionGateError>;
}

/// `ask_user_question` — 向用户提结构化多选题以澄清模糊需求。
pub struct AskUserQuestionTool;

/// 解析入参里的 questions（同时做 schema 校验）。
fn parse_questions(input: &Value) -> Result<Vec<Question>, String> {
    let arr = input
        .get("questions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "`questions` 必须是数组".to_string())?;
    if arr.is_empty() || arr.len() > 4 {
        return Err("questions 数量必须为 1-4".into());
    }
    let mut seen_q = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(arr.len());
    for q in arr {
        let question = q
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if question.is_empty() {
            return Err("每题必须有非空 question".into());
        }
        if !seen_q.insert(question.to_string()) {
            return Err("题干必须互不重复".into());
        }
        let header = q
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if header.is_empty() {
            return Err("每题必须有非空 header".into());
        }
        let multi_select = q
            .get("multi_select")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let opts = q
            .get("options")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "每题必须有 options 数组".to_string())?;
        if opts.len() < 2 || opts.len() > 4 {
            return Err("每题 options 数量必须为 2-4".into());
        }
        let mut seen_label = std::collections::HashSet::new();
        let mut options = Vec::with_capacity(opts.len());
        for o in opts {
            let label = o.get("label").and_then(|v| v.as_str()).unwrap_or("").trim();
            if label.is_empty() {
                return Err("每个选项必须有非空 label".into());
            }
            if !seen_label.insert(label.to_string()) {
                return Err("同一题内选项 label 必须互不重复".into());
            }
            let description = o
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            options.push(QuestionOption {
                label: label.to_string(),
                description,
            });
        }
        out.push(Question {
            header: header.to_string(),
            question: question.to_string(),
            multi_select,
            options,
        });
    }
    Ok(out)
}

/// 把答案组装成回灌给模型的 tool_result 文本。
fn format_answers(answers: &[AnswerItem]) -> String {
    let mut parts = Vec::new();
    for a in answers {
        let mut chosen = a.selected.join(", ");
        if let Some(other) = &a.other {
            if !other.trim().is_empty() {
                if chosen.is_empty() {
                    chosen = format!("其他: {}", other.trim());
                } else {
                    chosen = format!("{}, 其他: {}", chosen, other.trim());
                }
            }
        }
        if chosen.is_empty() {
            chosen = "(未作答)".into();
        }
        parts.push(format!("\"{}\"=\"{}\"", a.question, chosen));
    }
    format!("用户已回答你的问题：{}。可据此继续。", parts.join("，"))
}

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "ask_user_question"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn requires_user_interaction(&self) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        // 等用户答题 —— 给足时间，gate 自身另有 5min 超时。
        Duration::from_secs(360)
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        parse_questions(input).map(|_| ())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let questions = parse_questions(&args).map_err(|m| ToolError::InvalidArgs {
            tool: "ask_user_question".into(),
            message: m,
        })?;
        let gate = ctx.question_gate.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("问答 gate 不可用（无前端交互通道）".into())
        })?;
        let tool_use_id = ctx.current_tool_use_id.clone().unwrap_or_default();
        let req = QuestionRequest {
            thread_id: ctx.thread_id.clone().unwrap_or_default(),
            tool_use_id,
            questions,
        };
        match gate.ask(req, ctx.abort.clone()).await {
            Ok(QuestionOutcome::Answered(answers)) => Ok(format_answers(&answers)),
            Ok(QuestionOutcome::Cancelled) => {
                Ok("用户取消了澄清提问，未提供答案。请基于现有信息继续，或换个方式询问。".into())
            }
            Err(QuestionGateError::Aborted) => Err(ToolError::Aborted),
            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
        }
    }

    fn spec(&self) -> Option<ToolSpec> {
        Some(ToolSpec {
            tool_type: "function".into(),
            function: FunctionSpec {
                name: "ask_user_question".into(),
                description: "向用户提出结构化多选题，用于澄清模糊需求、收集偏好、在实现方案之间做决策或提供方向选择。\
何时使用：(1) 收集用户偏好/需求；(2) 澄清模糊或有歧义的指令；(3) 执行中需要在多个实现方案间做决策；(4) 给用户提供方向选择。\
规则：用户永远能选\"其他\"自由填空，所以不要自己在 options 里塞\"其他\"选项；若有推荐项放第一个并在 label 末尾加\"(推荐)\"；选项不互斥时用 multi_select。\
克制：优先自行做出合理推断；仅当歧义会导致返工、或需要用户做主观/不可逆决策时才调用；同一需求不要反复追问，一次问清相关的若干点（最多 4 题）。\
轮数可控：用户可在提示词里指定澄清的深入轮数（例如\"再细化三轮\"\"多问几轮\"），此时按要求连续多次调用本工具逐轮深入——上一轮答复到手后，基于答复继续发起下一轮更具体的问题，直到达到用户要求的轮数或需求已足够清晰；用户未指定轮数时，默认一轮问清即可。"
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "questions": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 4,
                            "description": "要问用户的题（1-4）",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "header": { "type": "string", "description": "极短标签（chip），如\"目标平台\"" },
                                    "question": { "type": "string", "description": "完整问题，以问号结尾" },
                                    "multi_select": { "type": "boolean", "description": "允许多选（默认 false）" },
                                    "options": {
                                        "type": "array",
                                        "minItems": 2,
                                        "maxItems": 4,
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "label": { "type": "string", "description": "选项展示文本（1-5 词）" },
                                                "description": { "type": "string", "description": "选项含义/后果说明" }
                                            },
                                            "required": ["label", "description"]
                                        }
                                    }
                                },
                                "required": ["header", "question", "options"]
                            }
                        }
                    },
                    "required": ["questions"]
                }),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn question_request_serializes_camelcase() {
        let req = QuestionRequest {
            thread_id: "t1".into(),
            tool_use_id: "u1".into(),
            questions: vec![Question {
                header: "目标平台".into(),
                question: "跑在哪个平台？".into(),
                multi_select: false,
                options: vec![
                    QuestionOption {
                        label: "Web".into(),
                        description: "网页".into(),
                    },
                    QuestionOption {
                        label: "桌面".into(),
                        description: "原生".into(),
                    },
                ],
            }],
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["threadId"], "t1");
        assert_eq!(v["toolUseId"], "u1");
        assert_eq!(v["questions"][0]["multiSelect"], false);
        assert_eq!(v["questions"][0]["options"][0]["label"], "Web");
    }

    #[test]
    fn question_answers_deserializes_camelcase() {
        let json = json!({
            "toolUseId": "u1",
            "cancelled": false,
            "answers": [ { "question": "跑在哪个平台？", "selected": ["Web"], "other": null } ]
        });
        let ans: QuestionAnswers = serde_json::from_value(json).unwrap();
        assert_eq!(ans.tool_use_id, "u1");
        assert!(!ans.cancelled);
        assert_eq!(ans.answers[0].selected, vec!["Web".to_string()]);
    }

    fn valid_input() -> serde_json::Value {
        json!({
            "questions": [{
                "header": "平台",
                "question": "跑在哪个平台？",
                "options": [
                    { "label": "Web", "description": "网页" },
                    { "label": "桌面", "description": "原生" }
                ]
            }]
        })
    }

    #[tokio::test]
    async fn validate_rejects_empty_questions() {
        let t = AskUserQuestionTool;
        assert!(t.validate_input(&json!({ "questions": [] })).await.is_err());
    }

    #[tokio::test]
    async fn validate_rejects_too_few_options() {
        let t = AskUserQuestionTool;
        let bad = json!({ "questions": [{ "header": "h", "question": "q?",
            "options": [{ "label": "only", "description": "d" }] }] });
        assert!(t.validate_input(&bad).await.is_err());
    }

    #[tokio::test]
    async fn validate_rejects_duplicate_question_text() {
        let t = AskUserQuestionTool;
        let opt =
            json!([{ "label": "a", "description": "d" }, { "label": "b", "description": "d" }]);
        let bad = json!({ "questions": [
            { "header": "h", "question": "same?", "options": opt },
            { "header": "h2", "question": "same?", "options": opt }
        ] });
        assert!(t.validate_input(&bad).await.is_err());
    }

    #[tokio::test]
    async fn validate_rejects_duplicate_option_labels() {
        let t = AskUserQuestionTool;
        let bad = json!({ "questions": [{ "header": "h", "question": "q?",
            "options": [{ "label": "x", "description": "d" }, { "label": "x", "description": "d2" }] }] });
        assert!(t.validate_input(&bad).await.is_err());
    }

    #[tokio::test]
    async fn validate_accepts_valid() {
        let t = AskUserQuestionTool;
        assert!(t.validate_input(&valid_input()).await.is_ok());
    }

    #[tokio::test]
    async fn execute_without_gate_errors() {
        let t = AskUserQuestionTool;
        let ctx = ToolContext::standalone();
        let err = t.execute(valid_input(), &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    #[test]
    fn tool_has_spec_and_is_read_only() {
        let t = AskUserQuestionTool;
        assert!(t.is_read_only());
        assert!(t.requires_user_interaction());
        let spec = t.spec().unwrap();
        assert_eq!(spec.function.name, "ask_user_question");
    }

    struct MockGate(QuestionOutcome);
    #[async_trait]
    impl QuestionGate for MockGate {
        async fn ask(
            &self,
            _req: QuestionRequest,
            _abort: CancellationToken,
        ) -> Result<QuestionOutcome, QuestionGateError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn execute_formats_answers() {
        use std::sync::Arc;
        let t = AskUserQuestionTool;
        let mut ctx = ToolContext::standalone();
        ctx.question_gate = Some(Arc::new(MockGate(QuestionOutcome::Answered(vec![
            AnswerItem {
                question: "跑在哪个平台？".into(),
                selected: vec!["Web".into()],
                other: None,
            },
        ]))));
        let out = t.execute(valid_input(), &ctx).await.unwrap();
        assert!(out.contains("跑在哪个平台？"));
        assert!(out.contains("Web"));
    }

    #[tokio::test]
    async fn execute_cancelled_reports() {
        use std::sync::Arc;
        let t = AskUserQuestionTool;
        let mut ctx = ToolContext::standalone();
        ctx.question_gate = Some(Arc::new(MockGate(QuestionOutcome::Cancelled)));
        let out = t.execute(valid_input(), &ctx).await.unwrap();
        assert!(out.contains("取消") || out.to_lowercase().contains("cancel"));
    }
}
