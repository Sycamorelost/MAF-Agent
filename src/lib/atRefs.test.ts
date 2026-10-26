import { describe, it, expect } from 'vitest';
import { registerAtRef, resolveAtRefs } from './atRefs';

// registry 为模块级单例,各用例使用互不相同的前缀避免相互污染
describe('atRefs', () => {
  it('已注册占位标记被替换为「出处行 + 片段」', () => {
    registerAtRef('[@note:标题A]', '知识库 / 标题A', '片段正文A');
    const out = resolveAtRefs('请看 [@note:标题A] 的内容');
    expect(out).toBe('请看 【引用片段 · 出处:知识库 / 标题A】\n片段正文A 的内容');
  });

  it('未注册的占位标记保留原文', () => {
    const out = resolveAtRefs('手输的 [@note:未注册] 不动');
    expect(out).toBe('手输的 [@note:未注册] 不动');
  });

  it('文本不含 [@ 时原样返回', () => {
    expect(resolveAtRefs('普通文本')).toBe('普通文本');
    expect(resolveAtRefs('')).toBe('');
  });

  it('同一占位标记出现多次时全部替换', () => {
    registerAtRef('[@memory:会话B]', '记忆 / 会话B', '片段B');
    const out = resolveAtRefs('[@memory:会话B] 与 [@memory:会话B]');
    const count = out.split('【引用片段 · 出处:记忆 / 会话B】').length - 1;
    expect(count).toBe(2);
  });

  it('重复注册同一占位标记时后写覆盖先写', () => {
    registerAtRef('[@note:覆盖C]', '来源1', '旧片段');
    registerAtRef('[@note:覆盖C]', '来源2', '新片段');
    const out = resolveAtRefs('[@note:覆盖C]');
    expect(out).toContain('来源2');
    expect(out).toContain('新片段');
  });

  it('注册超过 200 条时最早的一条被淘汰(FIFO)', () => {
    registerAtRef('[@note:first]', 'S', 'first-snippet');
    for (let i = 0; i < 200; i++) {
      registerAtRef(`[@note:fill-${i}]`, 'S', `snip-${i}`);
    }
    // 第 201 条挤掉 first
    expect(resolveAtRefs('[@note:first]')).toBe('[@note:first]');
    // 最后注册的仍在
    expect(resolveAtRefs('[@note:fill-199]')).toContain('snip-199');
  });

  it('占位标记含正则特殊字符时按字面量匹配(不受正则影响)', () => {
    registerAtRef('[@note:a(b).*]', 'S', '特殊字符片段');
    const out = resolveAtRefs('x [@note:a(b).*] y');
    expect(out).toContain('特殊字符片段');
  });
});
