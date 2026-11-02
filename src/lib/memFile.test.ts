import { describe, expect, it } from 'vitest';
import type { Frontmatter } from './ipc';
import { bodyOf, renderFull } from './memFile';

const fm = (patch: Partial<Frontmatter> = {}): Frontmatter => ({
  conversation: '测试会话',
  id: 'deadbeef',
  created: '2026-01-01T00:00:00+08:00',
  updated: '2026-01-02T00:00:00+08:00',
  memory_version: 3,
  models_used: ['gpt-5.2', 'claude-4'],
  ...patch,
});

describe('renderFull', () => {
  it('按固定顺序写入 frontmatter,正文以空行分隔', () => {
    const out = renderFull(fm(), '# 会话摘要\n\n内容');
    expect(out.startsWith('---\nconversation: "测试会话"\nid: deadbeef')).toBe(true);
    expect(out).toContain('models_used: ["gpt-5.2", "claude-4"]');
    expect(out.endsWith('---\n\n# 会话摘要\n\n内容')).toBe(true);
  });

  it('空可选字段不写入,额外字段用 JSON 序列化', () => {
    const out = renderFull(fm({ created: '', updated: '', memory_version: 0, models_used: [] }), 'body');
    expect(out).not.toContain('created:');
    expect(out).not.toContain('memory_version:');
    expect(out).not.toContain('models_used:');
    const withExtra = renderFull(fm({ extra_field: '自定义', count: 2 }), 'b');
    expect(withExtra).toContain('extra_field: "自定义"');
    expect(withExtra).toContain('count: 2');
  });
});

describe('bodyOf', () => {
  it('去掉 frontmatter 与前导空行', () => {
    const full = '---\nid: x\n---\n\n# 摘要\n\n内容';
    expect(bodyOf(full)).toBe('# 摘要\n\n内容');
  });

  it('无 frontmatter 时原样返回', () => {
    expect(bodyOf('# 摘要\n内容')).toBe('# 摘要\n内容');
  });

  it('frontmatter 未闭合时原样返回', () => {
    expect(bodyOf('---\nid: x\n正文')).toBe('---\nid: x\n正文');
  });
});
