//! submit_answers command — 前端把结构化问答答案投递给 QuestionGate。

use crate::dto::SubmitAnswersInput;
use crate::AppState;

/// 接收前端提交的问答答案并转发给 question gate。
///
/// 与 `approve_tool` 同构：通过 `tool_use_id` 唤醒在 [`crate::question_gate_impl::TauriQuestionGate`]
/// 里 parking 的请求。无匹配 pending 时记录告警（answer-before-register 由 gate
/// 内部 stash 兜底）。
#[tauri::command]
pub async fn submit_answers(
    state: tauri::State<'_, AppState>,
    input: SubmitAnswersInput,
) -> Result<(), String> {
    let answers = deepseek_tools::QuestionAnswers {
        tool_use_id: input.tool_use_id.clone(),
        cancelled: input.cancelled,
        answers: input.answers,
    };
    let delivered = state
        .question_gate
        .feed_response(&input.tool_use_id, answers);
    if !delivered {
        tracing::warn!(
            tool_use_id = %input.tool_use_id,
            "submit_answers: no pending request matched",
        );
    }
    Ok(())
}
