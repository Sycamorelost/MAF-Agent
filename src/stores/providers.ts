import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface ProvidersState {
  templates: ipc.ProviderTemplate[];
  /** provider_id -> 是否已配 key */
  hasKey: Record<string, boolean>;
  /** 当前全局选中的 provider_id + model */
  current: { providerId: string; model: string } | null;
  /** 单模型配置(迭代 D8) */
  modelConfig: ipc.ProviderModelConfig;
  load: () => Promise<void>;
  setKey: (providerId: string, key: string) => Promise<void>;
  removeKey: (providerId: string) => Promise<void>;
  select: (providerId: string, model: string) => void;
  /** 新增/更新模板后刷新 */
  saveTemplate: (template: ipc.ProviderTemplate) => Promise<ipc.ProviderTemplate>;
  /** 删除模板(含内置)后刷新 */
  deleteTemplate: (providerId: string) => Promise<void>;
  /** 补回被删除的内置模板后刷新 */
  restoreBuiltins: () => Promise<void>;
  /** 从厂家拉取模型列表并刷新模板 */
  fetchModels: (providerId: string) => Promise<string[]>;
  /** 加载单模型配置 */
  loadModelConfig: () => Promise<void>;
  /** 更新某模型覆盖项(局部合并后整体落盘) */
  updateModelOverride: (
    providerId: string,
    model: string,
    patch: Partial<ipc.ModelOverride>,
  ) => Promise<void>;
}

export const useProvidersStore = create<ProvidersState>((set, get) => ({
  templates: [],
  hasKey: {},
  current: null,
  modelConfig: {},

  load: async () => {
    const templates = await ipc.listProviderTemplates();
    const storedIds = await ipc.listStoredProviderIds();
    const hasKey: Record<string, boolean> = {};
    for (const t of templates) hasKey[t.id] = storedIds.includes(t.id);
    set({ templates, hasKey });
    // 当前选择可能因「停用/删除模板/清除 Key」而失效,失效则回退到首个可用模型(P3)
    const ready = templates.filter((t) => hasKey[t.id] && t.models.length > 0);
    const cur = get().current;
    const stillValid =
      !!cur && ready.some((t) => t.id === cur.providerId && t.models.includes(cur.model));
    if (!stillValid) {
      set({
        current: ready.length > 0 ? { providerId: ready[0].id, model: ready[0].models[0] } : null,
      });
    }
  },

  setKey: async (providerId, key) => {
    await ipc.saveApiKey(providerId, key);
    set((s) => ({ hasKey: { ...s.hasKey, [providerId]: true } }));
  },

  removeKey: async (providerId) => {
    await ipc.deleteApiKey(providerId);
    set((s) => ({ hasKey: { ...s.hasKey, [providerId]: false } }));
  },

  select: (providerId, model) => set({ current: { providerId, model } }),

  saveTemplate: async (template) => {
    const saved = await ipc.saveProviderTemplate(template);
    await get().load();
    return saved;
  },

  deleteTemplate: async (providerId) => {
    await ipc.deleteProviderTemplate(providerId);
    if (get().current?.providerId === providerId) set({ current: null });
    await get().load();
  },

  restoreBuiltins: async () => {
    await ipc.restoreBuiltinTemplates();
    await get().load();
  },

  fetchModels: async (providerId) => {
    const models = await ipc.fetchProviderModels(providerId);
    await get().load();
    return models;
  },

  loadModelConfig: async () => {
    const modelConfig = await ipc.getProviderModelConfig();
    set({ modelConfig });
  },

  updateModelOverride: async (providerId, model, patch) => {
    const cfg = get().modelConfig;
    const prev = cfg[providerId]?.[model] ?? ipc.EMPTY_MODEL_OVERRIDE;
    const next: ipc.ProviderModelConfig = {
      ...cfg,
      [providerId]: { ...(cfg[providerId] ?? {}), [model]: { ...prev, ...patch } },
    };
    set({ modelConfig: next });
    await ipc.saveProviderModelConfig(next);
    // 显示名/排序/启用会影响模板模型列表,刷新一次
    await get().load();
  },
}));
