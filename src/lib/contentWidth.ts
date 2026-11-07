//! 对话正文 / 输入卡列宽 —— 两者共用 --content-w,保证正文与输入框永远等宽。
//! 用户值存 localStorage,启动时写回 --content-w;设置页「外观与首页」实时调整。

const KEY = 'maf.contentWidth';

export const CONTENT_W_DEFAULT = 720;
export const CONTENT_W_MIN = 560;
export const CONTENT_W_MAX = 1000;
export const CONTENT_W_STEP = 20;

function clamp(v: number): number {
  const snapped = Math.round(v / CONTENT_W_STEP) * CONTENT_W_STEP;
  return Math.min(CONTENT_W_MAX, Math.max(CONTENT_W_MIN, snapped));
}

/** 读取已保存的正文宽度;缺失或非法时回落默认 720px。 */
export function loadContentWidth(): number {
  const raw = Number(localStorage.getItem(KEY));
  return Number.isFinite(raw) && raw > 0 ? clamp(raw) : CONTENT_W_DEFAULT;
}

/** 应用正文宽度:写 CSS 变量并持久化,返回实际生效值。 */
export function applyContentWidth(px: number): number {
  const v = clamp(px);
  document.documentElement.style.setProperty('--content-w', `${v}px`);
  localStorage.setItem(KEY, String(v));
  return v;
}
