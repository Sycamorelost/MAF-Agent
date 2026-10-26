import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  guiGetPolicy,
  guiSavePolicy,
  guiListWindows,
  guiFindElements,
  type GuiPolicy,
} from './gui';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);

const POLICY: GuiPolicy = {
  enabled: true,
  allowed_apps: ['notepad.exe'],
  element_ttl_secs: 30,
  max_elements: 200,
  max_text_len: 500,
};

describe('gui invoke 参数组装', () => {
  beforeEach(() => mockInvoke.mockReset());

  it('读取 / 保存策略', async () => {
    mockInvoke.mockResolvedValue(POLICY as never);
    const p = await guiGetPolicy();
    expect(mockInvoke).toHaveBeenCalledWith('gui_get_policy');
    expect(p).toEqual(POLICY);

    await guiSavePolicy(POLICY);
    expect(mockInvoke).toHaveBeenCalledWith('gui_save_policy', { policy: POLICY });
  });

  it('列出窗口', async () => {
    mockInvoke.mockResolvedValue([] as never);
    await guiListWindows();
    expect(mockInvoke).toHaveBeenCalledWith('gui_list_windows');
  });

  it('查找元素:缺省选项全部传 null', async () => {
    mockInvoke.mockResolvedValue([] as never);
    await guiFindElements(12345);
    expect(mockInvoke).toHaveBeenCalledWith('gui_find_elements', {
      hwnd: 12345,
      name: null,
      automationId: null,
      controlType: null,
      limit: null,
    });
  });

  it('查找元素:显式选项原样传递,未给的补 null', async () => {
    mockInvoke.mockResolvedValue([] as never);
    await guiFindElements(1, { name: '确定', limit: 10 });
    expect(mockInvoke).toHaveBeenCalledWith('gui_find_elements', {
      hwnd: 1,
      name: '确定',
      automationId: null,
      controlType: null,
      limit: 10,
    });
  });
});
