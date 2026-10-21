//! 服务商管理数据源:模板 / API Key 槽位 / 轮换 / 单模型配置。
//!
//! 从 SettingsPage 抽出(§10.2.1),页面只负责展示与 props 组装。

import { useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from '../../lib/ipc';

export function useProviderAdmin() {
  const [templates, setTemplates] = useState<ipc.ProviderTemplate[]>([]);
  const [storedIds, setStoredIds] = useState<string[]>([]);
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [savingKey, setSavingKey] = useState<string | null>(null);

  // API 流转:多 Key 槽位 + 轮换配置
  const [rotation, setRotation] = useState<ipc.RotationConfigs>({});
  const [slotKeys, setSlotKeys] = useState<Record<string, ipc.KeySlotView[]>>({});

  // 模板增删(用户拥有完全权限:内置模板亦可删除,也可自行新增)
  const [savingTpl, setSavingTpl] = useState<string | null>(null);
  const [tplMsg, setTplMsg] = useState<string | null>(null);
  const [addingTpl, setAddingTpl] = useState(false);
  const [newTpl, setNewTpl] = useState({ display_name: '', base_url: '', modelsText: '' });

  // 单模型配置(迭代 D8)
  const [modelConfig, setModelConfig] = useState<ipc.ProviderModelConfig>({});
  /** 与 modelConfig 同步的即时快照:连续编辑时避免读到过期 state 而互相覆盖 */
  const modelConfigRef = useRef<ipc.ProviderModelConfig>({});
  const [cards, setCards] = useState<ipc.ModelCard[]>([]);
  const [configMsg, setConfigMsg] = useState<string | null>(null);
  const [fetching, setFetching] = useState<Record<string, boolean>>({});
  const [fetchMsg, setFetchMsg] = useState<Record<string, string>>({});

  /** 拉取某 Provider 的 Key 槽位列表(掩码 / 备注 / 启用 / 冷却)。 */
  const loadSlots = async (providerId: string) => {
    try {
      const list = await ipc.listProviderKeys(providerId);
      setSlotKeys((m) => ({ ...m, [providerId]: list }));
    } catch {
      /* 忽略:渲染时按空列表降级 */
    }
  };

  // 加载(顺序与原 SettingsPage 初始化 effect 一致)
  useEffect(() => {
    ipc.listProviderTemplates().then(setTemplates);
    ipc.listStoredProviderIds().then((ids) => {
      setStoredIds(ids);
      // 预加载各 Provider 的 Key 槽位(掩码/备注/启用/冷却),供多 Key 面板展示
      ids.forEach((id) => void loadSlots(id));
    });
    ipc.getRotationConfigs().then(setRotation).catch(() => {});
    ipc.getProviderModelConfig().then((c) => {
      modelConfigRef.current = c;
      setModelConfig(c);
    }).catch(() => {});
    ipc.listModelCards().then(setCards).catch(() => {});
  }, []);

  const keyedTemplates = useMemo(
    () => templates.filter((t) => storedIds.includes(t.id)),
    [templates, storedIds],
  );

  /** 新增一把 Key(自动分配到新槽位;备注写入槽位元数据)。 */
  const onSaveKey = async (providerId: string, label: string) => {
    const key = keyInputs[providerId] ?? '';
    if (!key) return;
    setSavingKey(providerId);
    try {
      await ipc.saveProviderKey(providerId, key, label);
      setStoredIds((ids) => (ids.includes(providerId) ? ids : [...ids, providerId]));
      setKeyInputs((m) => ({ ...m, [providerId]: '' }));
      await loadSlots(providerId);
      setRotation(await ipc.getRotationConfigs());
    } finally {
      setSavingKey(null);
    }
  };

  /** 更新槽位元数据(备注 / 启用 / 权重 / 优先级;未传字段保持原值)。 */
  const onPatchSlot = async (
    providerId: string,
    slot: number,
    patch: { label?: string; enabled?: boolean; weight?: number; priority?: number },
  ) => {
    const cur = (slotKeys[providerId] ?? []).find((s) => s.slot === slot);
    await ipc.updateKeySlot(
      providerId,
      slot,
      patch.label ?? cur?.label ?? '',
      patch.enabled ?? cur?.enabled ?? true,
      patch.weight ?? cur?.weight ?? 1,
      patch.priority ?? cur?.priority ?? 0,
    );
    await loadSlots(providerId);
  };

  /** 删除单个槽位的 Key。 */
  const onDeleteSlot = async (providerId: string, slot: number) => {
    if (!confirm(`删除槽位 #${slot} 的 API Key?`)) return;
    await ipc.deleteProviderKey(providerId, slot);
    await loadSlots(providerId);
    setStoredIds(await ipc.listStoredProviderIds());
  };

  /** 更新轮换配置(启用 / 冷却 / stream_options 兼容开关)。 */
  const onPatchRotation = async (
    providerId: string,
    patch: { enabled?: boolean; cooldown_secs?: number; supports_stream_options?: boolean },
  ) => {
    const cur = rotation[providerId];
    const next = {
      enabled: patch.enabled ?? cur?.enabled ?? false,
      cooldown_secs: patch.cooldown_secs ?? cur?.cooldown_secs ?? 60,
      supports_stream_options: patch.supports_stream_options ?? cur?.supports_stream_options ?? true,
    };
    await ipc.setProviderRotation(
      providerId,
      next.enabled,
      next.cooldown_secs,
      next.supports_stream_options,
    );
    setRotation(await ipc.getRotationConfigs());
  };

  const onClearKey = async (providerId: string) => {
    if (!confirm(`清除 ${providerId} 的全部 API Key?`)) return;
    await ipc.deleteApiKey(providerId);
    setStoredIds((ids) => ids.filter((x) => x !== providerId));
    await loadSlots(providerId);
  };

  /** 重新加载模型卡(拉取厂家模型/手动保存价格后调用)。 */
  const reloadCards = async () => {
    try {
      setCards(await ipc.listModelCards());
    } catch {
      // 忽略:卡片为展示增强,不影响主流程
    }
  };

  const onFetchModels = async (providerId: string) => {
    setFetching((s) => ({ ...s, [providerId]: true }));
    setFetchMsg((s) => ({ ...s, [providerId]: '' }));
    try {
      const models = await ipc.fetchProviderModels(providerId);
      setFetchMsg((s) => ({
        ...s,
        [providerId]: `已拉取 ${models.length} 个模型(含额度/价格等元数据,已设为该服务商模型列表)`,
      }));
      setTemplates(await ipc.listProviderTemplates());
      // 厂家元数据 + 刊例目录派生的卡片需要同步刷新
      await reloadCards();
    } catch (e) {
      setFetchMsg((s) => ({
        ...s,
        [providerId]: `拉取失败:${e instanceof Error ? e.message : String(e)}`,
      }));
    } finally {
      setFetching((s) => ({ ...s, [providerId]: false }));
    }
  };

  /** 更新某模型覆盖项:本地即时生效 + 整体落盘 + 刷新模板列表(启用/排序影响列表)。 */
  const onPatchOverride = async (
    providerId: string,
    model: string,
    patch: Partial<ipc.ModelOverride>,
  ) => {
    const cur = modelConfigRef.current;
    const prev = cur[providerId]?.[model] ?? ipc.EMPTY_MODEL_OVERRIDE;
    const next: ipc.ProviderModelConfig = {
      ...cur,
      [providerId]: { ...(cur[providerId] ?? {}), [model]: { ...prev, ...patch } },
    };
    modelConfigRef.current = next;
    setModelConfig(next);
    setConfigMsg(null);
    try {
      await ipc.saveProviderModelConfig(next);
      setTemplates(await ipc.listProviderTemplates());
      setConfigMsg('设置已保存');
    } catch (e) {
      setConfigMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  /** 就地编辑某服务商(显示名 / Base URL / 模型列表),整体落盘后刷新。 */
  const onEditTemplate = async (providerId: string, patch: Partial<ipc.ProviderTemplate>) => {
    const tpl = templates.find((t) => t.id === providerId);
    if (!tpl) return;
    setSavingTpl(providerId);
    setTplMsg(null);
    try {
      await ipc.saveProviderTemplate({ ...tpl, all_models: [], ...patch });
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg('已保存');
    } catch (e) {
      setTplMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 新增服务商(OpenAI 兼容协议,id 由后端生成)。 */
  const onAddTemplate = async () => {
    setSavingTpl('__new__');
    setTplMsg(null);
    try {
      const saved = await ipc.saveProviderTemplate({
        id: '',
        display_name: newTpl.display_name.trim() || '新服务商',
        base_url: newTpl.base_url.trim(),
        models: newTpl.modelsText.split('\n').map((s) => s.trim()).filter(Boolean),
        all_models: [],
        image_models: [],
        docs_url: null,
        openai_compat: true,
      });
      setTemplates(await ipc.listProviderTemplates());
      setNewTpl({ display_name: '', base_url: '', modelsText: '' });
      setAddingTpl(false);
      setTplMsg(`已新增服务商「${saved.display_name}」`);
    } catch (e) {
      setTplMsg(`新增失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 删除服务商(内置模板亦可删除),同时清理其 Key、单模型配置与模型缓存。 */
  const onDeleteTemplate = async (providerId: string, name: string) => {
    if (!confirm(`删除服务商「${name}」?将同时清除其 API Key、单模型配置与模型缓存。`)) return;
    setSavingTpl(providerId);
    setTplMsg(null);
    try {
      await ipc.deleteProviderTemplate(providerId);
      setStoredIds((ids) => ids.filter((x) => x !== providerId));
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg(`已删除「${name}」`);
    } catch (e) {
      setTplMsg(`删除失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 补回被删除的内置模板(仅补缺失项,不影响用户已编辑/新增的模板)。 */
  const onRestoreBuiltins = async () => {
    setTplMsg(null);
    try {
      await ipc.restoreBuiltinTemplates();
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg('已补回缺失的内置模板');
    } catch (e) {
      setTplMsg(`恢复失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return {
    templates,
    storedIds,
    keyInputs,
    setKeyInputs,
    savingKey,
    rotation,
    slotKeys,
    savingTpl,
    tplMsg,
    addingTpl,
    setAddingTpl,
    newTpl,
    setNewTpl,
    modelConfig,
    cards,
    configMsg,
    fetching,
    fetchMsg,
    keyedTemplates,
    onSaveKey,
    onPatchSlot,
    onDeleteSlot,
    onPatchRotation,
    onClearKey,
    reloadCards,
    onFetchModels,
    onPatchOverride,
    onEditTemplate,
    onAddTemplate,
    onDeleteTemplate,
    onRestoreBuiltins,
  };
}
