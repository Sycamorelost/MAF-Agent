import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../lib/ipc', () => ({
  readMemory: vi.fn(),
  writeMemory: vi.fn(),
  listMemoryVersions: vi.fn(),
  rollbackMemory: vi.fn(),
  watchMemory: vi.fn(),
  unwatchMemory: vi.fn(),
}));

import * as ipc from '../lib/ipc';
import { useMemoryStore } from './memory';

const DTO: ipc.MemoryDto = {
  frontmatter: {
    conversation: 'c',
    id: 's1',
    created: 't',
    models_used: [],
    memory_version: 1,
    updated: 't',
  },
  body: '正文',
};

const reset = () =>
  useMemoryStore.setState({ bySession: {}, versions: {}, watching: new Set<string>() });

describe('memory store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it('load 缓存指定会话的记忆', async () => {
    vi.mocked(ipc.readMemory).mockResolvedValueOnce(DTO);
    await useMemoryStore.getState().load('s1');
    expect(useMemoryStore.getState().bySession['s1']).toEqual(DTO);
    expect(ipc.readMemory).toHaveBeenCalledWith('s1');
  });

  it('write 落盘后重新加载该会话', async () => {
    vi.mocked(ipc.writeMemory).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.readMemory).mockResolvedValueOnce(DTO);
    await useMemoryStore.getState().write('s1', '新内容', '备注');
    expect(ipc.writeMemory).toHaveBeenCalledWith('s1', '新内容', '备注');
    expect(useMemoryStore.getState().bySession['s1']).toEqual(DTO);
  });

  it('rollback 重新加载内容与版本列表', async () => {
    vi.mocked(ipc.rollbackMemory).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.readMemory).mockResolvedValueOnce(DTO);
    vi.mocked(ipc.listMemoryVersions).mockResolvedValueOnce([{ id: 2, created: 't', note: '' }]);
    await useMemoryStore.getState().rollback('s1', 2);
    expect(ipc.rollbackMemory).toHaveBeenCalledWith('s1', 2);
    expect(useMemoryStore.getState().versions['s1']).toHaveLength(1);
  });

  it('startWatch 幂等:同一会话只 watch 一次', async () => {
    vi.mocked(ipc.watchMemory).mockResolvedValue(undefined as never);
    await useMemoryStore.getState().startWatch('s1');
    await useMemoryStore.getState().startWatch('s1');
    expect(ipc.watchMemory).toHaveBeenCalledTimes(1);
    expect(useMemoryStore.getState().watching.has('s1')).toBe(true);
  });

  it('stopWatch 仅在监听中才调用 ipc,并移出集合', async () => {
    vi.mocked(ipc.watchMemory).mockResolvedValue(undefined as never);
    vi.mocked(ipc.unwatchMemory).mockResolvedValue(undefined as never);
    // 未监听时 stopWatch 不发 ipc
    await useMemoryStore.getState().stopWatch('s1');
    expect(ipc.unwatchMemory).not.toHaveBeenCalled();

    await useMemoryStore.getState().startWatch('s1');
    await useMemoryStore.getState().stopWatch('s1');
    expect(ipc.unwatchMemory).toHaveBeenCalledWith('s1');
    expect(useMemoryStore.getState().watching.size).toBe(0);
  });
});
