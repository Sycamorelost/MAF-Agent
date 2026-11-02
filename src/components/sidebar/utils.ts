/** Sidebar 纯函数 + localStorage 持久化工具 */

export const LS_COLLAPSED = 'metaagent.sidebar.collapsed';
export const LS_VIEW = 'metaagent.sidebar.view';
export type ViewMode = 'time' | 'project';

export function readCollapsed(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(LS_COLLAPSED);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}
export function writeCollapsed(v: Record<string, boolean>) {
  try {
    localStorage.setItem(LS_COLLAPSED, JSON.stringify(v));
  } catch { /* ignore */ }
}
export function readView(): ViewMode {
  try {
    const v = localStorage.getItem(LS_VIEW) as ViewMode | null;
    return v === 'time' ? 'time' : 'project';
  } catch {
    return 'project';
  }
}
export function writeView(v: ViewMode) {
  try {
    localStorage.setItem(LS_VIEW, v);
  } catch { /* ignore */ }
}

/** 时间标签:今天 → HH:mm;近 7 天 → 周X;更早 → M/D */
export function whenLabel(updated: string): string {
  const d = new Date(updated.replace(' ', 'T'));
  if (Number.isNaN(d.getTime())) return '';
  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  if (d.getTime() >= today) {
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }
  if (d.getTime() >= today - 6 * 86400_000) {
    return `周${'日一二三四五六'[d.getDay()]}`;
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}
