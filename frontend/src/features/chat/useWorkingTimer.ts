import { useEffect, useRef, useState } from "react";

/** 超过该时长（毫秒）没有新进展，视为"卡顿"，提示文案转琥珀色。 */
export const STALL_THRESHOLD_MS = 10_000;

export interface WorkingTimerState {
  /** 已经过的秒数（整数）。 */
  elapsedSec: number;
  /** 是否进入卡顿态（超过阈值无新进展）。 */
  stalled: boolean;
}

/**
 * 进行时计时器 —— 在 agent 工作期间每秒自增，给用户"没卡死、已经花了 Ns"的
 * 明确反馈。`progressKey` 每次有新进展（新 token / 新工具事件）时变化，用于
 * 重置卡顿判定：长时间不变 → stalled=true。
 *
 * @param active 是否正在工作（pendingTurn）。false 时停表并归零。
 * @param progressKey 进展信号；变化即重置计时基准与卡顿判定。
 */
export function useWorkingTimer(
  active: boolean,
  progressKey: number,
): WorkingTimerState {
  const [elapsedSec, setElapsedSec] = useState(0);
  const [stalled, setStalled] = useState(false);
  const lastProgressRef = useRef<number>(Date.now());

  // 进展信号变化 → 重置卡顿基准。
  useEffect(() => {
    lastProgressRef.current = Date.now();
    setStalled(false);
  }, [progressKey]);

  useEffect(() => {
    if (!active) {
      setElapsedSec(0);
      setStalled(false);
      lastProgressRef.current = Date.now();
      return;
    }
    const start = Date.now();
    const id = window.setInterval(() => {
      const now = Date.now();
      setElapsedSec(Math.floor((now - start) / 1000));
      setStalled(now - lastProgressRef.current > STALL_THRESHOLD_MS);
    }, 1000);
    return () => window.clearInterval(id);
  }, [active]);

  return { elapsedSec, stalled };
}
