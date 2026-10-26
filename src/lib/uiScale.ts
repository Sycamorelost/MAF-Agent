//! 全局字号缩放 —— 仅缩放字号 token(-font-*),不影响间距与布局。
//! 用户值存 localStorage,启动时写回 --ui-scale;设置页「字体大小」实时调整。

const KEY = 'maf.uiScale';

export const UI_SCALE_DEFAULT = 1;
export const UI_SCALE_MIN = 0.85;
export const UI_SCALE_MAX = 1.3;

function clamp(v: number): number {
  return Math.min(UI_SCALE_MAX, Math.max(UI_SCALE_MIN, Math.round(v * 100) / 100));
}

/** 读取已保存的缩放比例;缺失或非法时回落 1。 */
export function loadUiScale(): number {
  const raw = Number(localStorage.getItem(KEY));
  return Number.isFinite(raw) && raw > 0 ? clamp(raw) : UI_SCALE_DEFAULT;
}

/** 应用缩放:写 CSS 变量并持久化,返回实际生效值。 */
export function applyUiScale(scale: number): number {
  const v = clamp(scale);
  document.documentElement.style.setProperty('--ui-scale', String(v));
  localStorage.setItem(KEY, String(v));
  return v;
}
