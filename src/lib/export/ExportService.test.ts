import { describe, expect, it } from 'vitest';

import { inlineMd } from './ExportService';

describe('inlineMd 链接安全', () => {
  it('保留 http/https/mailto 与相对路径链接', () => {
    expect(inlineMd('[官网](https://example.com)')).toBe(
      '<a href="https://example.com">官网</a>',
    );
    expect(inlineMd('[写信](mailto:a@b.c)')).toBe('<a href="mailto:a@b.c">写信</a>');
    expect(inlineMd('[报告](./report.md)')).toBe('<a href="./report.md">报告</a>');
  });

  it('丢弃 javascript:/data: 等白名单外协议,只保留文字', () => {
    expect(inlineMd('[x](javascript:void)')).toBe('<span>x</span>');
    expect(inlineMd('[x](data:text/html;base64,AAA)')).toBe('<span>x</span>');
    // 制表符绕过(`java\tscript:`)同样拦截
    expect(inlineMd('[x](java\tscript:void)')).toBe('<span>x</span>');
  });

  it('转义引号,不能从 URL 里逃出属性', () => {
    const out = inlineMd('[x](https://a.com/?q="><img>)');
    expect(out).not.toContain('"><img');
    expect(out).toContain('&quot;');
  });

  it('文本本身仍按 HTML 实体转义', () => {
    expect(inlineMd('<b>粗</b> 与 `code`')).toBe(
      '&lt;b&gt;粗&lt;/b&gt; 与 <code>code</code>',
    );
  });
});
