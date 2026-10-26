import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: unknown;
  },
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

import { formatTokens, vendorCardId, COST_TIER_LABEL } from './provider';

describe('formatTokens', () => {
  it('null / undefined 显示为 -', () => {
    expect(formatTokens(null)).toBe('-');
    expect(formatTokens(undefined)).toBe('-');
  });

  it('小于 1000 直接返回数字', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
  });

  it('千级格式化为 K,整千不带小数', () => {
    expect(formatTokens(1000)).toBe('1K');
    expect(formatTokens(32_000)).toBe('32K');
    expect(formatTokens(1500)).toBe('1.5K');
  });

  it('百万级格式化为 M', () => {
    expect(formatTokens(1_000_000)).toBe('1M');
    expect(formatTokens(1_500_000)).toBe('1.5M');
  });
});

describe('vendorCardId', () => {
  it('拼接 provider 与 model', () => {
    expect(vendorCardId('openai', 'gpt-5')).toBe('vendor-openai-gpt-5');
  });

  it('model 中的非法字符替换为 -', () => {
    expect(vendorCardId('kimi', 'kimi k3/pro')).toBe('vendor-kimi-kimi-k3-pro');
  });

  it('保留字母数字与 ._-', () => {
    expect(vendorCardId('p', 'a.b_c-d1')).toBe('vendor-p-a.b_c-d1');
  });
});

describe('COST_TIER_LABEL', () => {
  it('五个成本档位均有中文名', () => {
    expect(COST_TIER_LABEL.free).toBe('免费');
    expect(COST_TIER_LABEL.budget).toBe('经济');
    expect(COST_TIER_LABEL.standard).toBe('标准');
    expect(COST_TIER_LABEL.premium).toBe('高端');
    expect(COST_TIER_LABEL.ultra).toBe('旗舰');
  });
});
