import { describe, it, expect } from 'vitest';
import { withProtocol, DEFAULT_SYSTEM_PROMPT, AGENT_PROTOCOL } from './systemPrompt';

describe('withProtocol', () => {
  it('空核心提示回退到默认提示 + 能力协议', () => {
    const out = withProtocol('');
    expect(out).toBe(`${DEFAULT_SYSTEM_PROMPT}\n\n${AGENT_PROTOCOL}`);
  });

  it('纯空白核心提示同样回退默认', () => {
    expect(withProtocol('   \n ')).toBe(`${DEFAULT_SYSTEM_PROMPT}\n\n${AGENT_PROTOCOL}`);
  });

  it('自定义核心提示追加能力协议', () => {
    const out = withProtocol('你是翻译助手');
    expect(out.startsWith('你是翻译助手')).toBe(true);
    expect(out).toContain(AGENT_PROTOCOL);
  });

  it('核心提示首尾空白被裁剪', () => {
    const out = withProtocol('  你是翻译助手  ');
    expect(out.startsWith('你是翻译助手')).toBe(true);
  });

  it('已包含能力协议标记时不重复追加', () => {
    const core = `自定义前缀\n\n${AGENT_PROTOCOL}`;
    expect(withProtocol(core)).toBe(core);
  });

  it('默认提示与协议内容非空且含关键硬约束', () => {
    expect(DEFAULT_SYSTEM_PROMPT.length).toBeGreaterThan(0);
    expect(AGENT_PROTOCOL).toContain('## 能力协议');
    expect(AGENT_PROTOCOL).toContain('连续失败 3 次');
  });
});
