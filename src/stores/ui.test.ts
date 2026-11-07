import { describe, it, expect } from 'vitest';
import { useUiStore } from './ui';

// node 环境无 localStorage,readFlag/writeFlag 内部 try/catch 兜底,状态仍可正常切换
describe('ui store', () => {
  it('侧栏默认展开', () => {
    expect(useUiStore.getState().sidebarOpen).toBe(true);
    expect(useUiStore.getState().sidebarWidth).toBe(280);
  });

  it('setSidebarOpen 切换状态', () => {
    useUiStore.getState().setSidebarOpen(true);
    expect(useUiStore.getState().sidebarOpen).toBe(true);
    useUiStore.getState().setSidebarOpen(false);
    expect(useUiStore.getState().sidebarOpen).toBe(false);
  });

  it('setSidebarWidth 钳制在 200–320', () => {
    useUiStore.getState().setSidebarWidth(100);
    expect(useUiStore.getState().sidebarWidth).toBe(200);
    useUiStore.getState().setSidebarWidth(999);
    expect(useUiStore.getState().sidebarWidth).toBe(320);
    useUiStore.getState().setSidebarWidth(260);
    expect(useUiStore.getState().sidebarWidth).toBe(260);
  });
});
