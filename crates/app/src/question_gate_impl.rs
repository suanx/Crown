//! TauriQuestionGate — emit `question:request` 并等待前端 `submit_answers`。
//!
//! 骨架照抄 `gate_impl.rs` 的 [`crate::gate_impl::TauriPermissionGate`]：
//! DashMap 按 `tool_use_id` parking + oneshot + abort + 5min 超时 +
//! answer-before-register 竞态防护（pre_arrived stash + TTL）。
//!
//! 与权限审批语义隔离：独立事件名、独立数据形状、独立 gate 实例。

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use deepseek_tools::{
    QuestionAnswers, QuestionGate, QuestionGateError, QuestionOutcome, QuestionRequest,
};

/// gate 等待用户答复的最长时间，超时自动放弃，避免引擎线程无限阻塞。
const GATE_TIMEOUT: Duration = Duration::from_secs(300);

/// "提前到达"的答复（在 `ask()` 注册前就 submit 了）保留的有效期。
const PRE_ARRIVAL_TTL: Duration = Duration::from_secs(30);

/// Tauri 实现的 [`QuestionGate`]。
///
/// 每次 [`QuestionGate::ask`]：
/// 1. 在 `pending` 里按 `tool_use_id` 注册一个 [`oneshot::Sender`]。
/// 2. emit `question:request` 让前端渲染问答面板。
/// 3. 等待匹配的 `submit_answers` invoke、abort、或超时。
pub struct TauriQuestionGate {
    app: AppHandle,
    pending: Arc<DashMap<String, oneshot::Sender<QuestionAnswers>>>,
    /// 在 `ask()` 注册前就到达的答复，按 `tool_use_id` 暂存，带到达时间用于 TTL。
    pre_arrived: Arc<DashMap<String, (QuestionAnswers, Instant)>>,
}

impl TauriQuestionGate {
    /// 绑定到给定 [`AppHandle`]。
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Arc::new(DashMap::new()),
            pre_arrived: Arc::new(DashMap::new()),
        }
    }

    /// 投递前端答复。无 pending 时 stash 进 `pre_arrived`
    /// （answer-before-register 竞态），同样返回 `true`（答复已被接受，只是早到）。
    pub fn feed_response(&self, tool_use_id: &str, answers: QuestionAnswers) -> bool {
        if let Some((_, sender)) = self.pending.remove(tool_use_id) {
            return sender.send(answers).is_ok();
        }
        self.pre_arrived
            .insert(tool_use_id.to_string(), (answers, Instant::now()));
        true
    }

    /// 取出未过期的提前到达答复（命中即移除）。
    fn take_pre_arrived(&self, tool_use_id: &str) -> Option<QuestionAnswers> {
        let fresh = self
            .pre_arrived
            .get(tool_use_id)
            .map(|e| e.value().1.elapsed() < PRE_ARRIVAL_TTL)
            .unwrap_or(false);
        if fresh {
            self.pre_arrived.remove(tool_use_id).map(|(_, (a, _))| a)
        } else {
            self.pre_arrived.remove(tool_use_id);
            None
        }
    }

    /// 把前端答复（含取消标志）映射为 [`QuestionOutcome`]。
    fn to_outcome(answers: QuestionAnswers) -> QuestionOutcome {
        if answers.cancelled {
            QuestionOutcome::Cancelled
        } else {
            QuestionOutcome::Answered(answers.answers)
        }
    }
}

#[async_trait]
impl QuestionGate for TauriQuestionGate {
    async fn ask(
        &self,
        req: QuestionRequest,
        abort: CancellationToken,
    ) -> Result<QuestionOutcome, QuestionGateError> {
        let id = req.tool_use_id.clone();
        if abort.is_cancelled() {
            return Err(QuestionGateError::Aborted);
        }
        // Fast path：答复在注册前就到了。
        if let Some(a) = self.take_pre_arrived(&id) {
            return Ok(Self::to_outcome(a));
        }
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id.clone(), tx);
        // 注册后再查一次，闭合 take_pre_arrived 与 insert 之间的窗口。
        if let Some(a) = self.take_pre_arrived(&id) {
            self.pending.remove(&id);
            return Ok(Self::to_outcome(a));
        }
        if let Err(e) = self.app.emit("question:request", &req) {
            self.pending.remove(&id);
            return Err(QuestionGateError::Emit(e.to_string()));
        }
        let result = tokio::select! {
            biased;
            _ = abort.cancelled() => {
                self.pending.remove(&id);
                Err(QuestionGateError::Aborted)
            }
            _ = tokio::time::sleep(GATE_TIMEOUT) => {
                self.pending.remove(&id);
                tracing::warn!(tool_use_id = %id, "question gate timed out after 5min");
                Err(QuestionGateError::Closed)
            }
            r = rx => r.map(Self::to_outcome).map_err(|_| QuestionGateError::Closed),
        };
        self.pending.remove(&id);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期断言 [`TauriQuestionGate`] 满足 [`QuestionGate`]。
    #[allow(dead_code)]
    fn _is_gate(g: TauriQuestionGate) -> Box<dyn QuestionGate> {
        Box::new(g)
    }
}
