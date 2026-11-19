import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import type { SearchResults } from '../../lib/ipc';
import SearchResultList, { buildSearchItems } from './SearchResultList';

const RES: SearchResults = {
  sessions: [{ id: 's1', title: '季度复盘', updated: '2026-11-01T10:00:00Z' }],
  messages: [
    {
      session_id: 's1',
      session_title: '季度复盘',
      message_id: 9,
      role: 'user',
      snippet: '关于营收的讨论',
      created: '2026-11-01T10:00:00Z',
      branch_id: 'main',
    },
  ],
  memory: [{ session_id: 's1', session_title: '季度复盘', snippet: '长期偏好:简洁输出' }],
  notes: [{ id: 'n1', title: '营收笔记', snippet: '营收拆解方法', updated: '2026-11-02T10:00:00Z' }],
  attachments: [
    {
      path: 'D:\\att\\a.pdf',
      name: 'report.pdf',
      kind: 'pdf',
      session_id: 's1',
      session_title: '季度复盘',
      message_id: 3,
      branch_id: 'main',
      snippet: '本季度营收同比增长',
    },
  ],
};

const render = (items: ReturnType<typeof buildSearchItems>, extra: Record<string, unknown> = {}) =>
  renderToStaticMarkup(
    createElement(SearchResultList, {
      items,
      activeIdx: 0,
      onActive: () => {},
      onPick: () => {},
      query: '营收',
      ...extra,
    }),
  );

describe('全局搜索多源结果', () => {
  it('按会话 / 消息 / 记忆 / 知识库 / 附件顺序拍平', () => {
    const items = buildSearchItems(RES);
    expect(items.map((i) => i.kind)).toEqual([
      'session',
      'message',
      'memory',
      'note',
      'attachment',
    ]);
  });

  it('附件条目带类型标签与来源会话', () => {
    const items = buildSearchItems(RES);
    const att = items.find((i) => i.kind === 'attachment');
    expect(att && 'meta' in att ? att.meta : '').toContain('PDF');
    expect(att && 'meta' in att ? att.meta : '').toContain('来源:季度复盘');
  });

  it('渲染分组标题与数量,顺序与条目一致', () => {
    const html = render(buildSearchItems(RES));
    const labels = ['会话', '消息', '记忆', '知识库', '附件'];
    const positions = labels.map((l) => html.indexOf(`${l}<span`));
    expect(positions.every((p) => p >= 0)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
    expect(html).toContain('本季度营收同比增长');
  });

  it('空结果给出无匹配提示', () => {
    const html = render([]);
    expect(html).toContain('没有匹配');
  });

  it('加载中与失败态优先展示', () => {
    expect(render([], { loading: true })).toContain('搜索中');
    expect(render([], { error: '网络异常' })).toContain('搜索失败');
  });
});
