import { describe, it, expect } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { uiConfirm, ConfirmHost } from './confirmService';
import { ConfirmDialog } from './ConfirmDialog';

// 全局确认服务(node 环境:测安全契约 + 静态渲染分支;
// 交互点击链路靠 cargo/手动回归,jsdom 未引入)

describe('uiConfirm 安全契约', () => {
  it('Host 未挂载时一律按取消结算,绝不静默放行危险操作', async () => {
    // 注意:node 环境下 ConfirmHost 的 useEffect 不执行,push 始终为 null,
    // 正好覆盖「Host 缺失」这一兜底分支
    await expect(uiConfirm('确定删除?')).resolves.toBe(false);
    await expect(uiConfirm({ message: 'x', danger: true })).resolves.toBe(false);
  });

  it('并发多个请求且 Host 缺失时全部 resolve(false),不悬挂', async () => {
    const rs = await Promise.all([uiConfirm('a'), uiConfirm('b'), uiConfirm('c')]);
    expect(rs).toEqual([false, false, false]);
  });
});

describe('ConfirmHost 静态渲染', () => {
  it('无待决请求时不渲染任何内容', () => {
    expect(renderToStaticMarkup(createElement(ConfirmHost))).toBe('');
  });
});

describe('ConfirmDialog 渲染分支', () => {
  const noop = () => {};

  it('默认按钮文案与标题/正文渲染', () => {
    const html = renderToStaticMarkup(
      createElement(ConfirmDialog, {
        title: '删除词条',
        message: '删除后不可恢复',
        onConfirm: noop,
        onCancel: noop,
      }),
    );
    expect(html).toContain('dialog-mask');
    expect(html).toContain('删除词条');
    expect(html).toContain('删除后不可恢复');
    expect(html).toContain('确认');
    expect(html).toContain('取消');
  });

  it('danger 时确认按钮使用告警色', () => {
    const html = renderToStaticMarkup(
      createElement(ConfirmDialog, {
        title: 't',
        message: 'm',
        danger: true,
        confirmText: '删除',
        onConfirm: noop,
        onCancel: noop,
      }),
    );
    expect(html).toContain('var(--warn)');
    expect(html).toContain('删除');
  });

  it('busy 时按钮禁用并显示处理中', () => {
    const html = renderToStaticMarkup(
      createElement(ConfirmDialog, {
        title: 't',
        message: 'm',
        busy: true,
        onConfirm: noop,
        onCancel: noop,
      }),
    );
    expect(html).toContain('处理中…');
    expect(html.match(/disabled=""|disabled/g)?.length).toBeGreaterThanOrEqual(2);
  });
});
