import { invoke } from '@tauri-apps/api/core';

/** 后端「静默失败」计数(P2):事件发送 / 会话事件落库 / MCP 审计写入。 */
export interface FailureCounts {
  emit: number;
  event_log: number;
  audit: number;
}

/** 读取累计失败计数(只读;仅用于「关于」页在计数非 0 时提示)。 */
export const getFailureCounts = () => invoke<FailureCounts>('diagnostics_failure_counts');
