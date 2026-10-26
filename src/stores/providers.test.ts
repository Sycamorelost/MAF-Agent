import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../lib/ipc', () => ({
  listProviderTemplates: vi.fn(),
  listStoredProviderIds: vi.fn(),
  saveApiKey: vi.fn(),
  deleteApiKey: vi.fn(),
  saveProviderTemplate: vi.fn(),
  deleteProviderTemplate: vi.fn(),
  restoreBuiltinTemplates: vi.fn(),
  fetchProviderModels: vi.fn(),
  getProviderModelConfig: vi.fn(),
  saveProviderModelConfig: vi.fn(),
  EMPTY_MODEL_OVERRIDE: {
    enabled: null,
    display_name: null,
    order: null,
    temperature: null,
    max_tokens: null,
    top_p: null,
    memory_injection: null,
    default_reasoning: null,
  },
}));

import * as ipc from '../lib/ipc';
import { useProvidersStore } from './providers';

const TPL = (id: string, models: string[]): ipc.ProviderTemplate => ({
  id,
  display_name: id,
  base_url: '',
  models,
  all_models: models,
  image_models: [],
  docs_url: null,
  openai_compat: true,
});

const reset = () =>
  useProvidersStore.setState({ templates: [], hasKey: {}, current: null, modelConfig: {} });

describe('providers store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it('load:按已存 key 计算 hasKey,当前选择失效时回退到首个可用模型', async () => {
    useProvidersStore.setState({ current: { providerId: 'p2', model: 'm1' } });
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([TPL('p1', ['a']), TPL('p2', ['m1'])]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce(['p1']);
    await useProvidersStore.getState().load();
    const s = useProvidersStore.getState();
    expect(s.hasKey).toEqual({ p1: true, p2: false });
    // p2 无 key → 当前选择失效,回退 p1/a
    expect(s.current).toEqual({ providerId: 'p1', model: 'a' });
  });

  it('load:当前选择仍有效时保持不变', async () => {
    useProvidersStore.setState({ current: { providerId: 'p1', model: 'a' } });
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([TPL('p1', ['a', 'b'])]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce(['p1']);
    await useProvidersStore.getState().load();
    expect(useProvidersStore.getState().current).toEqual({ providerId: 'p1', model: 'a' });
  });

  it('load:无任何可用模型时 current 置 null', async () => {
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([TPL('p1', [])]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce(['p1']);
    await useProvidersStore.getState().load();
    expect(useProvidersStore.getState().current).toBeNull();
  });

  it('setKey / removeKey 更新 hasKey', async () => {
    vi.mocked(ipc.saveApiKey).mockResolvedValueOnce(undefined as never);
    await useProvidersStore.getState().setKey('p1', 'sk-x');
    expect(useProvidersStore.getState().hasKey['p1']).toBe(true);

    vi.mocked(ipc.deleteApiKey).mockResolvedValueOnce(undefined as never);
    await useProvidersStore.getState().removeKey('p1');
    expect(useProvidersStore.getState().hasKey['p1']).toBe(false);
  });

  it('select 直接设置当前选择', () => {
    useProvidersStore.getState().select('p1', 'a');
    expect(useProvidersStore.getState().current).toEqual({ providerId: 'p1', model: 'a' });
  });

  it('deleteTemplate:删除当前选中的 provider 时先清空 current', async () => {
    useProvidersStore.setState({ current: { providerId: 'p1', model: 'a' } });
    vi.mocked(ipc.deleteProviderTemplate).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce([]);
    await useProvidersStore.getState().deleteTemplate('p1');
    expect(ipc.deleteProviderTemplate).toHaveBeenCalledWith('p1');
    expect(useProvidersStore.getState().current).toBeNull();
  });

  it('updateModelOverride:局部合并补丁后整体落盘并刷新', async () => {
    useProvidersStore.setState({
      modelConfig: { p1: { a: { ...ipc.EMPTY_MODEL_OVERRIDE, temperature: 0.5 } } },
    });
    vi.mocked(ipc.saveProviderModelConfig).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([TPL('p1', ['a'])]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce([]);
    await useProvidersStore.getState().updateModelOverride('p1', 'a', { top_p: 0.9 });
    const saved = vi.mocked(ipc.saveProviderModelConfig).mock.calls[0][0];
    // 补丁合并:temperature 保留,top_p 更新
    expect(saved.p1.a.temperature).toBe(0.5);
    expect(saved.p1.a.top_p).toBe(0.9);
    expect(useProvidersStore.getState().modelConfig.p1.a.top_p).toBe(0.9);
  });

  it('updateModelOverride:无既有覆盖时基于 EMPTY_MODEL_OVERRIDE', async () => {
    vi.mocked(ipc.saveProviderModelConfig).mockResolvedValueOnce(undefined as never);
    vi.mocked(ipc.listProviderTemplates).mockResolvedValueOnce([]);
    vi.mocked(ipc.listStoredProviderIds).mockResolvedValueOnce([]);
    await useProvidersStore.getState().updateModelOverride('p9', 'm9', { enabled: false });
    const saved = vi.mocked(ipc.saveProviderModelConfig).mock.calls[0][0];
    expect(saved.p9.m9.enabled).toBe(false);
    expect(saved.p9.m9.temperature).toBeNull();
  });
});
