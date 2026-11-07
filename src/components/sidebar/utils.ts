/** Sidebar 纯函数 + localStorage 持久化工具 */

import { loadUiScale } from '../../lib/uiScale';

export const LS_COLLAPSED = 'metaagent.sidebar.collapsed';
export const LS_PROJECTS_OPEN = 'metaagent.sidebar.projectsOpen';

/** 展开态栏宽约束(拖拽调整;双击复位到默认)。上下限随 --ui-scale 等比,避免大字体下栏宽不足。 */
export const SIDEBAR_W_MIN = 200;
export const SIDEBAR_W_MAX = 320;
export const SIDEBAR_W_DEFAULT = 280;

/** 默认宽度随字号缩放等比变化(读取当前用户缩放)。 */
export function defaultSidebarWidth(): number {
  return Math.round(SIDEBAR_W_DEFAULT * loadUiScale());
}

export function clampSidebarWidth(w: number): number {
  if (!Number.isFinite(w)) return defaultSidebarWidth();
  const s = loadUiScale();
  return Math.min(Math.round(SIDEBAR_W_MAX * s), Math.max(Math.round(SIDEBAR_W_MIN * s), Math.round(w)));
}

/**
 * 侧栏显示模式(与主内容同一图层,永不悬浮):
 * - expanded:与会话同层的常驻栏(推挤主区)
 * - hidden:完全折叠,不保留图标轨(通过标题栏按钮重新展开)
 */
export type SidebarMode = 'expanded' | 'hidden';

export function sidebarMode(open: boolean): SidebarMode {
  return open ? 'expanded' : 'hidden';
}

/** 侧栏分区:由当前路由决定,切换导航时列表内容随之切换。 */
export type SideSection = 'chat' | 'memory' | 'knowledge' | 'attachments';

export function sectionFromPath(pathname: string): SideSection {
  if (pathname.startsWith('/knowledge') || pathname.startsWith('/notes')) return 'knowledge';
  if (pathname.startsWith('/memory')) return 'memory';
  if (pathname.startsWith('/attachments')) return 'attachments';
  return 'chat';
}

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

/** 项目区块默认展开;显式折叠后记住。 */
export function readProjectsOpen(): boolean {
  try {
    return localStorage.getItem(LS_PROJECTS_OPEN) !== '0';
  } catch {
    return true;
  }
}
export function writeProjectsOpen(open: boolean) {
  try {
    localStorage.setItem(LS_PROJECTS_OPEN, open ? '1' : '0');
  } catch { /* ignore */ }
}

/** 跨平台绝对路径(Windows 盘符 / POSIX 根 / UNC)。 */
export function isAbsolutePath(path: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(path) || /^[\\/]/.test(path);
}

/** 一级导航选中判定(路径前缀;记忆/知识共用 /memory,靠 tab 参数区分)。 */
export function isNavActive(path: string, pathname: string, search = ''): boolean {
  if (path === '/') return pathname === '/' || pathname.startsWith('/chat');
  if (path === '/knowledge') {
    if (pathname === '/memory') {
      return new URLSearchParams(search).get('tab') === 'notes';
    }
    return pathname.startsWith('/knowledge') || pathname.startsWith('/notes');
  }
  if (path === '/memory' && pathname === '/memory') {
    return new URLSearchParams(search).get('tab') !== 'notes';
  }
  return pathname.startsWith(path);
}

export type TimeBucketKey = 'today' | 'yesterday' | 'week' | 'month' | 'older';

export const TIME_BUCKET_LABEL: Record<TimeBucketKey, string> = {
  today: '今天',
  yesterday: '昨天',
  week: '7 天内',
  month: '30 天内',
  older: '更早',
};

/** 会话归属的时间段(以本地零点为界,与「今天/昨天」直觉一致)。 */
export function timeBucket(updated: string, now = new Date()): TimeBucketKey {
  const d = new Date(updated.replace(' ', 'T'));
  if (Number.isNaN(d.getTime())) return 'older';
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  const t = start.getTime();
  if (d.getTime() >= t) return 'today';
  if (d.getTime() >= t - 86400_000) return 'yesterday';
  if (d.getTime() >= t - 7 * 86400_000) return 'week';
  if (d.getTime() >= t - 30 * 86400_000) return 'month';
  return 'older';
}

/** 按时间段分组;保持传入顺序,空分组不输出。 */
export function groupSessionsByTime<T extends { updated: string }>(
  list: T[],
  now = new Date(),
): { key: TimeBucketKey; label: string; items: T[] }[] {
  const order: TimeBucketKey[] = ['today', 'yesterday', 'week', 'month', 'older'];
  const buckets = new Map<TimeBucketKey, T[]>();
  for (const item of list) {
    const key = timeBucket(item.updated, now);
    const arr = buckets.get(key);
    if (arr) arr.push(item);
    else buckets.set(key, [item]);
  }
  return order
    .filter((key) => buckets.has(key))
    .map((key) => ({ key, label: TIME_BUCKET_LABEL[key], items: buckets.get(key)! }));
}

/** 侧栏搜索:按标题子串过滤(空查询返回原列表引用,避免无谓重渲染)。 */
export function filterSessionsByTitle<T extends { title: string }>(
  list: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return list;
  return list.filter((s) => (s.title || '').toLowerCase().includes(q));
}

/**
 * 项目分组搜索:命中项目名 → 保留该项目全部会话;
 * 否则只保留标题命中的会话,并丢弃没有任何命中会话的项目。
 */
export function filterProjectGroups<
  G extends { project: { name: string }; items: { title: string }[] },
>(groups: G[], query: string): G[] {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  return groups
    .filter((g) => {
      if ((g.project.name || '').toLowerCase().includes(q)) return true;
      return g.items.some((s) => (s.title || '').toLowerCase().includes(q));
    })
    .map((g) => {
      if ((g.project.name || '').toLowerCase().includes(q)) return g;
      return { ...g, items: filterSessionsByTitle(g.items, query) };
    });
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
