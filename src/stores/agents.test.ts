import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../lib/ipc', () => ({
  listAgentTemplates: vi.fn(),
  saveAgentTemplate: vi.fn(),
  deleteAgentTemplate: vi.fn(),
}));

import * as ipc from '../lib/ipc';
import { useAgentsStore } from './agents';

const AGENT = {
  agent_id: 'a1',
  name: '助手',
  icon: 'bot',
  description: '',
  category: 'chat',
  recommended_model: 'm',
  memory_strategy: 'full' as const,
  system_prompt: '',
  input_hints: [],
  memory_write: true,
  memory_sections: [],
  base_agent: null,
  builtin: false,
};

const reset = () => useAgentsStore.setState({ templates: [], loaded: false, loading: false });

describe('agents store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it('load 拉取模板并置 loaded', async () => {
    vi.mocked(ipc.listAgentTemplates).mockResolvedValueOnce([AGENT]);
    await useAgentsStore.getState().load();
    const s = useAgentsStore.getState();
    expect(s.templates).toEqual([AGENT]);
    expect(s.loaded).toBe(true);
    expect(s.loading).toBe(false);
  });

  it('loading 期间重复 load 被忽略(只发起一次 ipc)', async () => {
    let resolveFn!: (v: ipc.AgentTemplate[]) => void;
    vi.mocked(ipc.listAgentTemplates).mockReturnValueOnce(
      new Promise((r) => {
        resolveFn = r;
      }),
    );
    const p1 = useAgentsStore.getState().load();
    const p2 = useAgentsStore.getState().load();
    resolveFn([]);
    await Promise.all([p1, p2]);
    expect(ipc.listAgentTemplates).toHaveBeenCalledTimes(1);
  });

  it('load 异常时 loading 也会复位', async () => {
    vi.mocked(ipc.listAgentTemplates).mockRejectedValueOnce(new Error('ipc 失败'));
    await expect(useAgentsStore.getState().load()).rejects.toThrow('ipc 失败');
    expect(useAgentsStore.getState().loading).toBe(false);
  });

  it('save 先落盘再刷新列表', async () => {
    vi.mocked(ipc.saveAgentTemplate).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.listAgentTemplates).mockResolvedValueOnce([AGENT]);
    await useAgentsStore.getState().save(AGENT);
    expect(ipc.saveAgentTemplate).toHaveBeenCalledWith(AGENT);
    expect(useAgentsStore.getState().templates).toEqual([AGENT]);
  });

  it('remove 删除后刷新列表', async () => {
    useAgentsStore.setState({ templates: [AGENT], loaded: true });
    vi.mocked(ipc.deleteAgentTemplate).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.listAgentTemplates).mockResolvedValueOnce([]);
    await useAgentsStore.getState().remove('a1');
    expect(ipc.deleteAgentTemplate).toHaveBeenCalledWith('a1');
    expect(useAgentsStore.getState().templates).toEqual([]);
  });
});
