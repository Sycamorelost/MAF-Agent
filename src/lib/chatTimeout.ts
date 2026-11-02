//! 生成时长设置的纯逻辑(与后端 `settings.rs` 的边界保持一致,便于单测)。

/** 下限(分钟) */
export const MIN_CHAT_TIMEOUT_MINUTES = 1;
/** 上限(分钟):24 小时 */
export const MAX_CHAT_TIMEOUT_MINUTES = 1440;
/** 默认 10 分钟(与旧版固定值一致,升级后行为不变) */
export const DEFAULT_CHAT_TIMEOUT_MINUTES = 10;

/** 快捷档位(分钟):覆盖日常到超长工程。 */
export const TIMEOUT_PRESETS = [10, 30, 60, 120, 240, 720, 1440];

/** 夹到合法范围;非法/空值回落默认。 */
export function clampTimeoutMinutes(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_CHAT_TIMEOUT_MINUTES;
  return Math.min(MAX_CHAT_TIMEOUT_MINUTES, Math.max(MIN_CHAT_TIMEOUT_MINUTES, Math.round(n)));
}

/** 分钟数 → 便于阅读的文案(整小时附小时数)。 */
export function fmtTimeoutMinutes(n: number): string {
  return n % 60 === 0 ? `${n} 分钟(${n / 60} 小时)` : `${n} 分钟`;
}
