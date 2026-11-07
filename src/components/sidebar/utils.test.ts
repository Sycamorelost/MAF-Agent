import { describe, expect, it } from 'vitest';
import {
  clampSidebarWidth,
  filterProjectGroups,
  filterSessionsByTitle,
  groupSessionsByTime,
  isAbsolutePath,
  isNavActive,
  sectionFromPath,
  SIDEBAR_W_DEFAULT,
  sidebarMode,
  timeBucket,
} from './utils';

describe('sidebarMode', () => {
  it('展开 → expanded(同层常驻,推挤主区,不悬浮)', () => {
    expect(sidebarMode(true)).toBe('expanded');
  });

  it('折叠 → hidden(完全卸载,不保留图标轨)', () => {
    expect(sidebarMode(false)).toBe('hidden');
  });
});

describe('sectionFromPath', () => {
  it('按路由切换侧栏分区', () => {
    expect(sectionFromPath('/')).toBe('chat');
    expect(sectionFromPath('/chat/abc')).toBe('chat');
    expect(sectionFromPath('/memory')).toBe('memory');
    expect(sectionFromPath('/knowledge')).toBe('knowledge');
    expect(sectionFromPath('/notes')).toBe('knowledge');
    expect(sectionFromPath('/attachments')).toBe('attachments');
    expect(sectionFromPath('/settings')).toBe('chat');
  });
});

describe('clampSidebarWidth', () => {
  it('限制在 200-320 之间并取整', () => {
    expect(clampSidebarWidth(150)).toBe(200);
    expect(clampSidebarWidth(999)).toBe(320);
    expect(clampSidebarWidth(280.6)).toBe(281);
  });

  it('非法值回落到默认宽度', () => {
    expect(clampSidebarWidth(Number.NaN)).toBe(SIDEBAR_W_DEFAULT);
  });
});

describe('timeBucket', () => {
  const now = new Date('2026-11-06T12:00:00');

  it('按今天/昨天/7天内/30天内/更早分段', () => {
    expect(timeBucket('2026-11-06 09:00', now)).toBe('today');
    expect(timeBucket('2026-11-05 23:30', now)).toBe('yesterday');
    expect(timeBucket('2026-11-02 12:00', now)).toBe('week');
    expect(timeBucket('2026-10-15 12:00', now)).toBe('month');
    expect(timeBucket('2026-09-01 12:00', now)).toBe('older');
  });

  it('无法解析的时间归入更早,避免丢会话', () => {
    expect(timeBucket('not-a-date', now)).toBe('older');
    expect(timeBucket('', now)).toBe('older');
  });
});

describe('groupSessionsByTime', () => {
  const now = new Date('2026-11-06T12:00:00');

  it('分组保持传入顺序,空分组不输出', () => {
    const list = [
      { id: 'a', updated: '2026-11-06 10:00' },
      { id: 'b', updated: '2026-11-06 09:00' },
      { id: 'c', updated: '2026-10-15 09:00' },
    ];
    const groups = groupSessionsByTime(list, now);
    expect(groups.map((g) => g.key)).toEqual(['today', 'month']);
    expect(groups[0].label).toBe('今天');
    expect(groups[0].items.map((s) => s.id)).toEqual(['a', 'b']);
    expect(groups[1].items.map((s) => s.id)).toEqual(['c']);
  });
});

describe('filterSessionsByTitle', () => {
  const list = [
    { id: '1', title: '财务报表整理' },
    { id: '2', title: 'Rust Tauri 调试' },
    { id: '3', title: '' },
  ];

  it('空查询返回原列表', () => {
    expect(filterSessionsByTitle(list, '  ')).toBe(list);
  });

  it('按标题子串过滤,忽略大小写', () => {
    expect(filterSessionsByTitle(list, '报表').map((s) => s.id)).toEqual(['1']);
    expect(filterSessionsByTitle(list, 'tauri').map((s) => s.id)).toEqual(['2']);
    expect(filterSessionsByTitle(list, '不存在')).toEqual([]);
  });
});

describe('isNavActive', () => {
  it('对话页包含首页与 /chat 子路径', () => {
    expect(isNavActive('/', '/')).toBe(true);
    expect(isNavActive('/', '/chat/abc')).toBe(true);
    expect(isNavActive('/', '/memory')).toBe(false);
  });

  it('记忆库与知识库相互独立', () => {
    expect(isNavActive('/memory', '/memory')).toBe(true);
    expect(isNavActive('/memory', '/notes')).toBe(false);
    expect(isNavActive('/knowledge', '/knowledge')).toBe(true);
    // 旧知识库深链仍高亮知识库入口
    expect(isNavActive('/knowledge', '/notes')).toBe(true);
    // 共用 /memory 时靠 tab 参数区分
    expect(isNavActive('/knowledge', '/memory', '?tab=notes')).toBe(true);
    expect(isNavActive('/memory', '/memory', '?tab=notes')).toBe(false);
    expect(isNavActive('/memory', '/memory', '?tab=native')).toBe(true);
  });

  it('其余入口按路径前缀匹配', () => {
    expect(isNavActive('/attachments', '/attachments')).toBe(true);
    expect(isNavActive('/attachments', '/settings')).toBe(false);
  });
});

describe('filterProjectGroups', () => {
  const groups = [
    { project: { id: 'p1', name: '财务系统' }, items: [{ id: 's1', title: '报表整理' }] },
    { project: { id: 'p2', name: '研发' }, items: [{ id: 's2', title: 'Rust 调试' }] },
  ];

  it('空查询返回原数组引用', () => {
    expect(filterProjectGroups(groups, '  ')).toBe(groups);
  });

  it('命中项目名时保留该项目全部会话', () => {
    const out = filterProjectGroups(groups, '财务');
    expect(out.map((g) => g.project.id)).toEqual(['p1']);
    expect(out[0].items.map((s) => s.id)).toEqual(['s1']);
  });

  it('只命中会话标题时保留该项目及匹配项,并丢弃空项目', () => {
    const out = filterProjectGroups(groups, 'rust');
    expect(out.map((g) => g.project.id)).toEqual(['p2']);
    expect(out[0].items.map((s) => s.id)).toEqual(['s2']);
  });

  it('无匹配时返回空数组', () => {
    expect(filterProjectGroups(groups, '不存在')).toEqual([]);
  });
});

describe('isAbsolutePath', () => {
  it('接受 Windows 盘符、POSIX 根与 UNC', () => {
    expect(isAbsolutePath('C:\\work')).toBe(true);
    expect(isAbsolutePath('C:/work')).toBe(true);
    expect(isAbsolutePath('/home/user/work')).toBe(true);
    expect(isAbsolutePath('\\\\server\\share')).toBe(true);
  });

  it('拒绝相对路径与空串', () => {
    expect(isAbsolutePath('work/dir')).toBe(false);
    expect(isAbsolutePath('')).toBe(false);
  });
});
