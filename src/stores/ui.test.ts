import { describe, it, expect } from 'vitest';
import { useUiStore } from './ui';

// node 环境无 localStorage,readFlag/writeFlag 内部 try/catch 兜底,状态仍可正常切换
describe('ui store', () => {
  it('node 环境下初始值回退为 false', () => {
    expect(useUiStore.getState().sidebarOpen).toBe(false);
    expect(useUiStore.getState().asideOpen).toBe(false);
  });

  it('setSidebarOpen 切换状态', () => {
    useUiStore.getState().setSidebarOpen(true);
    expect(useUiStore.getState().sidebarOpen).toBe(true);
    useUiStore.getState().setSidebarOpen(false);
    expect(useUiStore.getState().sidebarOpen).toBe(false);
  });

  it('setAsideOpen 切换状态且不影响 sidebar', () => {
    useUiStore.getState().setSidebarOpen(true);
    useUiStore.getState().setAsideOpen(true);
    expect(useUiStore.getState().asideOpen).toBe(true);
    useUiStore.getState().setAsideOpen(false);
    expect(useUiStore.getState().sidebarOpen).toBe(true);
    useUiStore.getState().setSidebarOpen(false);
  });
});
