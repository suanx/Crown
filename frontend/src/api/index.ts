/**
 * ============================================================================
 * AgentClient 单例入口
 * ============================================================================
 *
 * UI 层只 import 这一个 agentClient.
 * 切换 mock / hybrid / tauri 通过 vite 环境变量:
 *
 *   VITE_API_MODE=mock            — 完全走 MockAgentClient
 *   VITE_API_MODE=hybrid          — HybridClient,按 CONTRACT_STATUS 自动分流
 *   VITE_API_MODE=tauri           — 完全走 TauriAgentClient
 *   未显式配置时: Tauri 桌面端走 tauri,普通浏览器走 mock.
 *
 * 也导出 contracts / status / devtools 供其它模块使用.
 * ----------------------------------------------------------------------------
 */

import type { AgentClient } from "./AgentClient";
import { MockAgentClient } from "./mock/MockAgentClient";
import { TauriAgentClient } from "./tauri/TauriAgentClient";
import { HybridClient } from "./HybridClient";

type ApiMode = "mock" | "hybrid" | "tauri";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

function detectDefaultMode(): ApiMode {
  if (typeof window !== "undefined" && window.__TAURI_INTERNALS__) {
    return "tauri";
  }
  return "mock";
}

const configuredMode = import.meta.env.VITE_API_MODE as ApiMode | undefined;
const mode: ApiMode = configuredMode || detectDefaultMode();

function build(): AgentClient {
  switch (mode) {
    case "tauri":
      return new TauriAgentClient();
    case "hybrid":
      return new HybridClient(new MockAgentClient(), new TauriAgentClient());
    case "mock":
    default:
      return new MockAgentClient();
  }
}

export const agentClient: AgentClient = build();

export const apiMode: ApiMode = mode;

// Re-exports
export * from "./contracts";
export * from "./AgentClient";
export { CONTRACT_STATUS, computeContractStats } from "./status";
export type { ContractStatus, ContractStats } from "./status";
export { devtools } from "./devtools";
export { assertShape, assertArrayShape } from "./assertShape";
