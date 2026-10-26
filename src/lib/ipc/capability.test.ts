import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  SCOPE_META,
  scopeLabel,
  mcpIssueCapability,
  mcpListCapabilities,
  mcpRevokeCapability,
  mcpClearCapabilities,
  type Scope,
} from './capability';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);

describe('SCOPE_META / scopeLabel', () => {
  it('五种作用域均有中文名与描述', () => {
    const ids = SCOPE_META.map((m) => m.id);
    expect(ids).toEqual(['fs_read', 'fs_write', 'exec', 'net', 'ui']);
    for (const m of SCOPE_META) {
      expect(m.label.length).toBeGreaterThan(0);
      expect(m.desc.length).toBeGreaterThan(0);
    }
  });

  it('已知作用域返回中文名', () => {
    expect(scopeLabel('fs_read')).toBe('文件只读');
    expect(scopeLabel('exec')).toBe('命令执行');
    expect(scopeLabel('ui')).toBe('GUI 自动化');
  });

  it('未知作用域回退到原始 id', () => {
    expect(scopeLabel('unknown' as Scope)).toBe('unknown');
  });
});

describe('capability invoke 参数组装', () => {
  beforeEach(() => mockInvoke.mockReset());

  it('签发令牌:省略 ttl / maxUses 时传 null', async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await mcpIssueCapability('测试', ['fs_read']);
    expect(mockInvoke).toHaveBeenCalledWith('mcp_issue_capability', {
      label: '测试',
      scopes: ['fs_read'],
      ttlSecs: null,
      maxUses: null,
    });
  });

  it('签发令牌:显式 ttl / maxUses 原样传递', async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await mcpIssueCapability('测试', ['net', 'ui'], 600, 5);
    expect(mockInvoke).toHaveBeenCalledWith('mcp_issue_capability', {
      label: '测试',
      scopes: ['net', 'ui'],
      ttlSecs: 600,
      maxUses: 5,
    });
  });

  it('列出 / 撤销 / 清空令牌调用对应命令', async () => {
    mockInvoke.mockResolvedValue([] as never);
    await mcpListCapabilities();
    expect(mockInvoke).toHaveBeenCalledWith('mcp_list_capabilities');

    await mcpRevokeCapability('cap-1');
    expect(mockInvoke).toHaveBeenCalledWith('mcp_revoke_capability', { id: 'cap-1' });

    await mcpClearCapabilities();
    expect(mockInvoke).toHaveBeenCalledWith('mcp_clear_capabilities');
  });
});
