import { describe, it, expect } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import Toast from './Toast';

// 仅验证渲染分支(node 环境用 renderToStaticMarkup,不需要 DOM)
describe('Toast', () => {
  it('单行提示渲染为轻量胶囊,不带清单面板与关闭按钮', () => {
    const html = renderToStaticMarkup(createElement(Toast, { text: '已保存' }));
    expect(html).toContain('toast');
    expect(html).not.toContain('toast-list');
    expect(html).not.toContain('toast-x');
  });

  it('多行清单渲染为可滚动面板并带关闭按钮', () => {
    const html = renderToStaticMarkup(
      createElement(Toast, { text: '「a.exe」为格式黑名单文件\n「b.dll」为空文件', onClose: () => {} }),
    );
    expect(html).toContain('toast-list');
    expect(html).toContain('toast-body');
    expect(html).toContain('toast-x');
    expect(html).toContain('b.dll');
  });

  it('空内容不渲染任何节点', () => {
    expect(renderToStaticMarkup(createElement(Toast, { text: null }))).toBe('');
  });

  it('极端数量(2000 条)也一次性渲染完整清单,不被截断', () => {
    const lines = Array.from({ length: 2000 }, (_, i) => `「f${i}.exe」为格式黑名单文件，无法拖入对话`);
    const html = renderToStaticMarkup(
      createElement(Toast, { text: lines.join('\n'), onClose: () => {} }),
    );
    expect(html).toContain('toast-body');
    expect(html.split('无法拖入对话').length - 1).toBe(2000);
    expect(html).toContain('f1999.exe');
  });
});
