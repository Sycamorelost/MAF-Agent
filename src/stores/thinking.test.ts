import { describe, it, expect, beforeEach } from 'vitest';
import { useThinkingStore, DEFAULT_THINKING_LEVEL } from './thinking';

const reset = () => useThinkingStore.setState({ bySession: {} });

describe('thinking store', () => {
  beforeEach(reset);

  it('init 缺省档位为全局默认 medium,开关为 off(需手动开启)', () => {
    useThinkingStore.getState().init('s1');
    expect(useThinkingStore.getState().bySession['s1']).toEqual({ on: false, level: 'medium' });
    expect(DEFAULT_THINKING_LEVEL).toBe('medium');
  });

  it('init 可指定默认档位', () => {
    useThinkingStore.getState().init('s1', 'high');
    expect(useThinkingStore.getState().bySession['s1']).toEqual({ on: false, level: 'high' });
  });

  it('init 可指定默认开启(智能体偏好)', () => {
    useThinkingStore.getState().init('s1', 'high', true);
    expect(useThinkingStore.getState().bySession['s1']).toEqual({ on: true, level: 'high' });
  });

  it('init 幂等:已有选择不被覆盖', () => {
    useThinkingStore.getState().init('s1');
    useThinkingStore.getState().set('s1', { on: false, level: 'ultra' });
    useThinkingStore.getState().init('s1', 'light');
    expect(useThinkingStore.getState().bySession['s1']).toEqual({ on: false, level: 'ultra' });
  });

  it('set 覆盖指定会话选择', () => {
    useThinkingStore.getState().init('s1');
    useThinkingStore.getState().set('s1', { on: true, level: 'light' });
    expect(useThinkingStore.getState().bySession['s1'].level).toBe('light');
  });

  it('多会话互不干扰', () => {
    useThinkingStore.getState().init('s1');
    useThinkingStore.getState().init('s2', 'ultra');
    useThinkingStore.getState().set('s1', { on: false, level: 'high' });
    const st = useThinkingStore.getState().bySession;
    expect(st['s1']).toEqual({ on: false, level: 'high' });
    expect(st['s2']).toEqual({ on: false, level: 'ultra' });
  });
});
