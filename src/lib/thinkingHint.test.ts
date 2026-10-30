import { describe, it, expect } from 'vitest';
import { effortHint, THINKING_LEVEL_LABEL } from './thinkingHint';

describe('THINKING_LEVEL_LABEL', () => {
  it('四档均有显示名', () => {
    expect(THINKING_LEVEL_LABEL).toEqual({
      light: '轻量',
      medium: '标准',
      high: '深入',
      ultra: '极限',
    });
  });
});

describe('effortHint', () => {
  it('openai:light/medium 映射同名,high/ultra 均为 high', () => {
    expect(effortHint('openai', 'gpt-5', 'light')).toBe('low');
    expect(effortHint('openai', 'gpt-5', 'medium')).toBe('medium');
    expect(effortHint('openai', 'gpt-5', 'high')).toBe('high');
    expect(effortHint('openai', 'gpt-5', 'ultra')).toBe('high');
  });

  it('deepseek:带 thinking 前缀,ultra 映射 max', () => {
    expect(effortHint('deepseek', 'deepseek-r1', 'light')).toBe('thinking + low');
    expect(effortHint('deepseek', 'deepseek-r1', 'ultra')).toBe('thinking + max');
  });

  it('kimi:K3 模型支持 effort 分档', () => {
    expect(effortHint('kimi', 'kimi-k3', 'medium')).toBe('effort high');
    expect(effortHint('kimi', 'kimi-k3', 'ultra')).toBe('effort max');
  });

  it('kimi:K2.6 仅开关,返回 thinking on', () => {
    expect(effortHint('kimi', 'kimi-k2.6', 'high')).toBe('thinking on');
  });

  it('kimi:模型名大小写不敏感', () => {
    expect(effortHint('kimi', 'KIMI-K3', 'light')).toBe('effort low');
  });

  it('qwen:四档映射 token 预算', () => {
    expect(effortHint('qwen', 'qwen3', 'light')).toBe('budget 2048');
    expect(effortHint('qwen', 'qwen3', 'medium')).toBe('budget 8192');
    expect(effortHint('qwen', 'qwen3', 'high')).toBe('budget 24576');
    expect(effortHint('qwen', 'qwen3', 'ultra')).toBe('budget 不限');
  });

  it('zhipu:glm-5.3 / glm-5.2 分档,其余仅开关', () => {
    expect(effortHint('zhipu', 'glm-5.3', 'ultra')).toBe('effort max');
    expect(effortHint('zhipu', 'glm-5.2', 'medium')).toBe('effort medium');
    expect(effortHint('zhipu', 'glm-4.6', 'high')).toBe('thinking on');
  });

  it('未知厂商走通用 reasoning_effort 映射(无 ultra 档)', () => {
    expect(effortHint('other-vendor', 'm', 'light')).toBe('reasoning_effort low');
    expect(effortHint('other-vendor', 'm', 'medium')).toBe('reasoning_effort medium');
    expect(effortHint('other-vendor', 'm', 'ultra')).toBe('reasoning_effort high');
  });
});
