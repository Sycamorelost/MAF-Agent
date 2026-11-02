import { describe, it, expect } from 'vitest';
import {
  DEFAULT_CHAT_TIMEOUT_MINUTES,
  MAX_CHAT_TIMEOUT_MINUTES,
  MIN_CHAT_TIMEOUT_MINUTES,
  clampTimeoutMinutes,
  fmtTimeoutMinutes,
} from './chatTimeout';

describe('生成时长设置', () => {
  it('默认 10 分钟,边界 1 与 1440', () => {
    expect(DEFAULT_CHAT_TIMEOUT_MINUTES).toBe(10);
    expect(MIN_CHAT_TIMEOUT_MINUTES).toBe(1);
    expect(MAX_CHAT_TIMEOUT_MINUTES).toBe(1440);
  });

  it('越界与非法值回落', () => {
    expect(clampTimeoutMinutes(0)).toBe(1);
    expect(clampTimeoutMinutes(-5)).toBe(1);
    expect(clampTimeoutMinutes(MAX_CHAT_TIMEOUT_MINUTES + 10)).toBe(MAX_CHAT_TIMEOUT_MINUTES);
    expect(clampTimeoutMinutes(Number.NaN)).toBe(DEFAULT_CHAT_TIMEOUT_MINUTES);
    expect(clampTimeoutMinutes(Infinity)).toBe(DEFAULT_CHAT_TIMEOUT_MINUTES);
  });

  it('常规值取整后原样保留', () => {
    expect(clampTimeoutMinutes(10)).toBe(10);
    expect(clampTimeoutMinutes(61.4)).toBe(61);
    expect(clampTimeoutMinutes(1440)).toBe(1440);
  });

  it('文案:整小时附小时数', () => {
    expect(fmtTimeoutMinutes(10)).toBe('10 分钟');
    expect(fmtTimeoutMinutes(60)).toBe('60 分钟(1 小时)');
    expect(fmtTimeoutMinutes(1440)).toBe('1440 分钟(24 小时)');
  });
});
