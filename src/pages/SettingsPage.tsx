import { Fragment, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getVersion } from '@tauri-apps/api/app';
import logoUrl from '../assets/logo.png';
import * as ipc from '../lib/ipc';
import AsideDrawer from '../components/AsideDrawer';
import { IconCheck, IconPlus, IconRefresh, IconTrash, IconUpload } from '../components/icons';

import { ENGINES, NAV, CN_NUM, TERMS, NAV_VISIBILITY } from './settings/constants';
import { ProviderSettingsCard } from './settings/ProviderSettingsCard';

export default function SettingsPage() {
  const [searchParams] = useSearchParams();
  const initialNav = searchParams.get('tab') ?? 'providers';
  const [nav, setNav] = useState(initialNav);
  // 关于页版本号:运行期读 tauri.conf.json(单一事实源,避免多处硬编码漂移)
  const [appVersion, setAppVersion] = useState('');
  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(''));
  }, []);

  // 已在设置页时,再次点击「设置」通过 query 切分区
  useEffect(() => {
    const tab = searchParams.get('tab');
    if (tab) setNav(tab);
  }, [searchParams]);

  const [templates, setTemplates] = useState<ipc.ProviderTemplate[]>([]);
  const [storedIds, setStoredIds] = useState<string[]>([]);
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [savingKey, setSavingKey] = useState<string | null>(null);

  // API 流转:多 Key 槽位 + 轮换配置
  const [rotation, setRotation] = useState<ipc.RotationConfigs>({});
  const [slotKeys, setSlotKeys] = useState<Record<string, ipc.KeySlotView[]>>({});

  // 用量统计
  const [usageDim, setUsageDim] = useState<'model' | 'provider' | 'key' | 'agent'>('model');
  const [usageDays, setUsageDays] = useState(30);
  const [usage, setUsage] = useState<ipc.UsageSummary | null>(null);
  const [usageList, setUsageList] = useState<ipc.UsageEntry[]>([]);
  const [usageHours, setUsageHours] = useState<ipc.UsageHour[]>([]);
  const [usageHealth, setUsageHealth] = useState<Record<string, ipc.UsageGroup[]>>({});
  /** 下钻:点分类条后按该分组键过滤明细表(null = 不过滤) */
  const [usageDrill, setUsageDrill] = useState<string | null>(null);

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

  const [settings, setSettings] = useState<ipc.AppSettings | null>(null);
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsSaved, setSettingsSaved] = useState(false);

  // 数据目录(可自定义)
  const [dataDir, setDataDir] = useState<ipc.DataDirInfo | null>(null);
  const [dirMsg, setDirMsg] = useState<string | null>(null);
  const [dirBusy, setDirBusy] = useState(false);
  const [dirRestarting, setDirRestarting] = useState(false);

  const [searchCfg, setSearchCfg] = useState<ipc.SearchConfigDto | null>(null);
  const [searchDraft, setSearchDraft] = useState({ engine: 'searxng', base_url: '', api_key: '' });
  const [savingSearch, setSavingSearch] = useState(false);
  const [searchTestStatus, setSearchTestStatus] = useState<string | null>(null);
  const [searchTesting, setSearchTesting] = useState(false);

  // 翻译术语表(迭代 D6.1)
  const [glossary, setGlossary] = useState<ipc.GlossaryEntry[] | null>(null);
  const [glossaryDirty, setGlossaryDirty] = useState(false);
  const [glossaryMsg, setGlossaryMsg] = useState<string | null>(null);
  const [savingGlossary, setSavingGlossary] = useState(false);
  const importRef = useRef<HTMLInputElement | null>(null);

  // 加载
  useEffect(() => {
    ipc.listProviderTemplates().then(setTemplates);
    ipc.listStoredProviderIds().then((ids) => {
      setStoredIds(ids);
      // 预加载各 Provider 的 Key 槽位(掩码/备注/启用/冷却),供多 Key 面板展示
      ids.forEach((id) => void loadSlots(id));
    });
    ipc.getRotationConfigs().then(setRotation).catch(() => {});
    ipc.getSettings().then((s) => setSettings(s));
    ipc.getDataDir().then(setDataDir).catch(() => {});
    ipc.getProviderModelConfig().then((c) => {
      modelConfigRef.current = c;
      setModelConfig(c);
    }).catch(() => {});
    ipc.listModelCards().then(setCards).catch(() => {});
    ipc.getSearchConfig().then((c) => {
      setSearchCfg(c);
      setSearchDraft((d) => ({ ...d, engine: c.engine, base_url: c.base_url }));
    });
    ipc.glossaryList().then(setGlossary);
  }, []);

  const keyedTemplates = useMemo(
    () => templates.filter((t) => storedIds.includes(t.id)),
    [templates, storedIds],
  );

  const engineLabel = ENGINES.find((e) => e.id === searchCfg?.engine)?.name ?? '未配置';
  const engineReady = searchCfg
    ? searchCfg.engine === 'searxng'
      ? !!searchCfg.base_url
      : searchCfg.has_api_key
    : false;

  /** 拉取某 Provider 的 Key 槽位列表(掩码 / 备注 / 启用 / 冷却)。 */
  const loadSlots = async (providerId: string) => {
    try {
      const list = await ipc.listProviderKeys(providerId);
      setSlotKeys((m) => ({ ...m, [providerId]: list }));
    } catch {
      /* 忽略:渲染时按空列表降级 */
    }
  };

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

  // ---- 用量统计 ----

  const loadUsage = async (dim: 'model' | 'provider' | 'key' | 'agent', days: number) => {
    const [sum, list, hours] = await Promise.all([
      ipc.usageSummary(dim, days),
      ipc.usageEntries(100),
      ipc.usageHourly(days),
    ]);
    setUsage(sum);
    setUsageList(list);
    setUsageHours(hours);
    // Key 健康度:按已有 Key 的 Provider 逐个拉取(供「Key 健康」表展示)
    const ids = await ipc.listStoredProviderIds();
    const pairs = await Promise.all(
      ids.map(async (id) => [id, await ipc.usageKeyHealth(id, days).catch(() => [])] as const),
    );
    setUsageHealth(Object.fromEntries(pairs));
  };

  useEffect(() => {
    if (nav !== 'usage') return;
    setUsageDrill(null);
    void loadUsage(usageDim, usageDays);
  }, [nav, usageDim, usageDays]);

  // ---- 单模型配置(迭代 D8)----

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

  // ---- 模板增删改(用户拥有完全权限:内置模板亦可删除)----

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

  const onSaveSettings = async () => {
    if (!settings) return;
    setSavingSettings(true);
    setSettingsSaved(false);
    try {
      await ipc.saveSettings(settings);
      setSettingsSaved(true);
    } finally {
      setSavingSettings(false);
    }
  };

  /** 更换数据目录:选目录 → 确认(提示旧目录将被删除) → 写配置 → 重启迁移。 */
  const onChangeDataDir = async () => {
    setDirMsg(null);
    try {
      const p = await ipc.pickDataDir();
      if (!p) return;
      if (dataDir && p.toLowerCase() === dataDir.path.toLowerCase()) {
        setDirMsg('所选目录与当前目录相同,无需更改');
        return;
      }
      const oldTip = dataDir?.path ? `\n\n迁移完成后原目录\n${dataDir.path}\n将被删除。` : '';
      if (!confirm(`确定把数据目录更改为:\n${p}${oldTip}\n\n应用将自动重启以完成迁移。`)) return;
      setDirBusy(true);
      const r = await ipc.setDataDir(p, true);
      if (r.restart_required) {
        setDirRestarting(true);
        await ipc.restartApp();
      } else {
        setDataDir(await ipc.getDataDir());
        setDirMsg('数据目录已更新(无需重启)');
      }
    } catch (e) {
      setDirMsg(`更改失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDirBusy(false);
    }
  };

  const onOpenDataDir = async () => {
    setDirMsg(null);
    try {
      await ipc.openDataDir();
    } catch (e) {
      setDirMsg(`打开失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const onSaveSearch = async () => {
    setSavingSearch(true);
    setSearchTestStatus(null);
    try {
      await ipc.saveSearchConfig({
        engine: searchDraft.engine,
        base_url: searchDraft.base_url,
        api_key: searchDraft.api_key || null,
      });
      const c = await ipc.getSearchConfig();
      setSearchCfg(c);
      setSearchDraft((d) => ({ ...d, base_url: c.base_url, api_key: '' }));
    } catch (e) {
      setSearchTestStatus(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingSearch(false);
    }
  };

  const onTestSearch = async () => {
    setSearchTesting(true);
    setSearchTestStatus('测试中…');
    try {
      const res = await ipc.searchTest('MAF Agent');
      if (res.length === 0) {
        setSearchTestStatus('测试成功,但未返回结果(可换关键词)');
      } else {
        const first = res[0];
        setSearchTestStatus(`测试成功,返回 ${res.length} 条;示例:${first.title || first.url}`);
      }
    } catch (e) {
      setSearchTestStatus(`测试失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSearchTesting(false);
    }
  };

  // ---- 术语表(迭代 D6.1)----

  const mutateGlossary = (next: ipc.GlossaryEntry[]) => {
    setGlossary(next);
    setGlossaryDirty(true);
    setGlossaryMsg(null);
  };

  const onSaveGlossary = async (next?: ipc.GlossaryEntry[]) => {
    const list = next ?? glossary ?? [];
    setSavingGlossary(true);
    setGlossaryMsg(null);
    try {
      await ipc.glossarySave(list);
      setGlossary(list.filter((e) => e.src.trim() && e.dst.trim()));
      setGlossaryDirty(false);
      setGlossaryMsg(`已保存 ${list.filter((e) => e.src.trim() && e.dst.trim()).length} 条术语`);
    } catch (e) {
      setGlossaryMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingGlossary(false);
    }
  };

  /** 导入 JSON 术语表(数组结构 [{src,dst,note?}];按 src 合并去重后立即保存)。 */
  const onImportGlossary = async (file: File) => {
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as unknown;
      if (!Array.isArray(parsed)) throw new Error('JSON 顶层必须是数组');
      const incoming: ipc.GlossaryEntry[] = [];
      for (const item of parsed) {
        const o = item as Record<string, unknown>;
        if (typeof o?.src === 'string' && typeof o?.dst === 'string' && o.src && o.dst) {
          incoming.push({
            src: o.src,
            dst: o.dst,
            note: typeof o.note === 'string' ? o.note : null,
          });
        }
      }
      if (incoming.length === 0) throw new Error('未解析到有效词条(需 src/dst 字段)');
      // 按 src 合并:新词条覆盖同名旧词条
      const map = new Map<string, ipc.GlossaryEntry>();
      for (const e of glossary ?? []) map.set(e.src, e);
      for (const e of incoming) map.set(e.src, e);
      const merged = [...map.values()];
      await onSaveGlossary(merged);
      setGlossaryMsg(`已导入 ${incoming.length} 条(合并后共 ${merged.length} 条)`);
    } catch (e) {
      setGlossaryMsg(`导入失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const providersView = (
    <>
      <h2>模型服务商</h2>
      <p className="set-desc">
        逐个服务商配置 API Key;「从厂家拉取」可获取最新模型列表(拉取成功后以厂家列表为准)。
        每个模型可单独启用/停用、改显示名与排序、设定默认生成参数、记忆注入策略与思考能力。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          服务商
          {configMsg || tplMsg ? (
            <span className="st">
              {configMsg}
              {configMsg && tplMsg ? ' · ' : ''}
              {tplMsg}
            </span>
          ) : null}
          <button
            className="btn-ghost"
            onClick={() => void onRestoreBuiltins()}
            title="仅补回被删除的内置模板,不影响已编辑/新增的模板"
          >
            <IconRefresh size={13} />
            恢复内置模板
          </button>
        </div>
        <div className="set-card-b">
          {templates.length === 0 && (
            <div className="set-hint">暂无服务商;可在下方「新增服务商」或「恢复内置模板」。</div>
          )}
          {templates.map((t) => (
            <ProviderSettingsCard
              key={t.id}
              tpl={t}
              hasKey={storedIds.includes(t.id)}
              keyInput={keyInputs[t.id] ?? ''}
              onKeyInput={(v) => setKeyInputs((m) => ({ ...m, [t.id]: v }))}
              onClearKey={() => void onClearKey(t.id)}
              savingKey={savingKey === t.id}
              overrides={modelConfig[t.id] ?? {}}
              cards={cards}
              onPatch={(model, patch) => void onPatchOverride(t.id, model, patch)}
              onFetch={() => void onFetchModels(t.id)}
              fetching={!!fetching[t.id]}
              fetchMsg={fetchMsg[t.id] ?? ''}
              savingTpl={savingTpl === t.id}
              onEdit={(patch) => void onEditTemplate(t.id, patch)}
              onDelete={() => void onDeleteTemplate(t.id, t.display_name)}
              rotation={rotation[t.id]}
              slots={slotKeys[t.id] ?? []}
              onAddKey={(label) => void onSaveKey(t.id, label)}
              onPatchSlot={(slot, patch) => void onPatchSlot(t.id, slot, patch)}
              onDeleteSlot={(slot) => void onDeleteSlot(t.id, slot)}
              onPatchRotation={(patch) => void onPatchRotation(t.id, patch)}
              onSaveCard={() => void reloadCards()}
            />
          ))}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          新增服务商
          <span className="set-hint" style={{ marginLeft: 'auto' }}>
            OpenAI 兼容协议
          </span>
        </div>
        <div className="set-card-b">
          {addingTpl ? (
            <>
              <div className="set-row">
                <label className="lbl">显示名称</label>
                <input
                  className="pg-field"
                  value={newTpl.display_name}
                  onChange={(e) => setNewTpl({ ...newTpl, display_name: e.target.value })}
                  placeholder="如 我的中转站"
                />
              </div>
              <div className="set-row">
                <label className="lbl">Base URL</label>
                <input
                  className="pg-field"
                  value={newTpl.base_url}
                  onChange={(e) => setNewTpl({ ...newTpl, base_url: e.target.value })}
                  placeholder="https://api.example.com/v1"
                />
              </div>
              <div className="set-row" style={{ alignItems: 'flex-start' }}>
                <label className="lbl" style={{ paddingTop: 6 }}>
                  模型列表
                </label>
                <textarea
                  className="pg-field"
                  rows={3}
                  value={newTpl.modelsText}
                  onChange={(e) => setNewTpl({ ...newTpl, modelsText: e.target.value })}
                  placeholder={'model-a\nmodel-b'}
                  style={{ resize: 'vertical' }}
                />
              </div>
              <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
                <button className="btn-ghost" onClick={() => setAddingTpl(false)}>
                  取消
                </button>
                <span className="sp" />
                <button
                  className="btn-primary"
                  onClick={() => void onAddTemplate()}
                  disabled={savingTpl === '__new__'}
                >
                  创建服务商
                </button>
              </div>
            </>
          ) : (
            <div className="set-actions">
              <span className="set-hint">
                可添加任意 OpenAI 兼容服务商;配置 Key 后即可在对话中选用。
              </span>
              <span className="sp" />
              <button className="btn-primary" onClick={() => setAddingTpl(true)}>
                <IconPlus size={14} />
                新增服务商
              </button>
            </div>
          )}
        </div>
      </div>
    </>
  );

  const memoryView = (
    <>
      <h2>记忆与提炼</h2>
      <p className="set-desc">对话后自动触发提炼;留空则跟随当前会话模型。</p>

      <div className="set-card">
        <div className="set-card-h">
          提炼模型
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">提炼 Provider</label>
            <select
              className="pg-field"
              value={settings?.summarize_provider_id ?? ''}
              onChange={(e) =>
                setSettings((s) =>
                  s ? { ...s, summarize_provider_id: e.target.value || null } : s,
                )
              }
            >
              <option value="">跟随当前会话模型</option>
              {keyedTemplates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.display_name}
                </option>
              ))}
            </select>
          </div>
          <div className="set-row">
            <label className="lbl">提炼模型</label>
            {(() => {
              const pid = settings?.summarize_provider_id ?? '';
              const tpl = keyedTemplates.find((t) => t.id === pid);
              const models = tpl?.models ?? [];
              const cur = settings?.summarize_model ?? '';
              // 已保存的值不在当前可选列表(模型被停用/Provider 已换)时保留显示,避免静默丢失
              const opts = cur && !models.includes(cur) ? [cur, ...models] : models;
              return (
                <select
                  className="pg-field"
                  value={cur}
                  disabled={!pid}
                  onChange={(e) =>
                    setSettings((s) => (s ? { ...s, summarize_model: e.target.value || null } : s))
                  }
                >
                  <option value="">{pid ? '跟随 Provider 默认模型' : '跟随当前会话模型'}</option>
                  {opts.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              );
            })()}
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSettings} disabled={savingSettings}>
              保存记忆设置
            </button>
          </div>
        </div>
      </div>
    </>
  );

  const searchView = (
    <>
      <h2>联网搜索</h2>
      <p className="set-desc">深度研究与对话联网的检索来源,国内渠道优先。</p>

      <div className="set-card">
        <div className="set-card-h">
          搜索引擎配置
          <span className={`st${engineReady ? '' : ' warn'}`}>
            {engineReady ? '已就绪' : '未就绪'} · {engineLabel}
          </span>
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">搜索引擎</label>
            <select
              className="pg-field"
              value={searchDraft.engine}
              onChange={(e) => setSearchDraft((d) => ({ ...d, engine: e.target.value }))}
            >
              {ENGINES.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.name}
                </option>
              ))}
            </select>
          </div>
          <div className="set-row">
            <label className="lbl">Base URL</label>
            <div style={{ flex: 1, minWidth: 0 }}>
              <input
                className="pg-field"
                value={searchDraft.base_url}
                onChange={(e) => setSearchDraft((d) => ({ ...d, base_url: e.target.value }))}
                placeholder="SearXNG 需填写实例地址;其余引擎可留空"
              />
            </div>
          </div>
          <div className="set-row">
            <label className="lbl">API Key</label>
            <input
              className="pg-field"
              type="password"
              value={searchDraft.api_key}
              onChange={(e) => setSearchDraft((d) => ({ ...d, api_key: e.target.value }))}
              placeholder={
                searchCfg?.has_api_key
                  ? '留空则保留已配置的 Key'
                  : searchDraft.engine === 'deepseek'
                    ? '留空则自动复用模型服务商中的 DeepSeek Key'
                    : 'SearXNG 可留空'
              }
            />
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button className="btn-ghost" onClick={onTestSearch} disabled={searchTesting}>
              {searchTesting ? '测试中…' : '测试搜索'}
            </button>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSearch} disabled={savingSearch}>
              保存搜索设置
            </button>
          </div>
          {searchTestStatus && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              {searchTestStatus}
            </div>
          )}
        </div>
      </div>
    </>
  );

  // 翻译视图(迭代 D6):术语表管理
  const translateView = (
    <>
      <h2>翻译</h2>
      <p className="set-desc">
        术语表在翻译时自动注入,保证专有名词译法一致;落盘 data/glossary.json。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          术语表
          <span className="st">
            <IconCheck size={13} />
            {glossary?.length ?? 0} 条
          </span>
        </div>
        <div className="set-card-b">
          {glossary === null && <div className="set-hint">加载中…</div>}
          {glossary !== null && glossary.length === 0 && (
            <div className="set-hint" style={{ marginBottom: 'var(--space-3)' }}>
              暂无术语。添加后,翻译原文命中「原文词」时将优先采用指定译法。
            </div>
          )}
          {glossary?.map((entry, idx) => (
            <div className="set-row" key={idx} style={{ marginBottom: 'var(--space-2)' }}>
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="原文词"
                value={entry.src}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, src: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="指定译法"
                value={entry.dst}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, dst: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="备注(可选)"
                value={entry.note ?? ''}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, note: e.target.value } : x)),
                  )
                }
              />
              <button
                className="btn-ghost"
                title="删除词条"
                onClick={() => mutateGlossary(glossary.filter((_, i) => i !== idx))}
              >
                <IconTrash size={14} />
              </button>
            </div>
          ))}
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button
              className="btn-ghost"
              onClick={() => mutateGlossary([...(glossary ?? []), { src: '', dst: '', note: '' }])}
            >
              <IconPlus size={14} />
              添加词条
            </button>
            <button className="btn-ghost" onClick={() => importRef.current?.click()}>
              <IconUpload size={14} />
              导入 JSON
            </button>
            <span className="sp" />
            <button
              className="btn-primary"
              onClick={() => void onSaveGlossary()}
              disabled={savingGlossary || !glossaryDirty}
            >
              {savingGlossary ? '保存中…' : glossaryDirty ? '保存术语表' : '已保存'}
            </button>
          </div>
          {glossaryMsg && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              {glossaryMsg}
            </div>
          )}
          <input
            ref={importRef}
            type="file"
            accept=".json,application/json"
            style={{ display: 'none' }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void onImportGlossary(f);
              e.target.value = '';
            }}
          />
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">DeepL 专用引擎</div>
        <div className="set-card-b">
          <div className="set-hint">
            设置页「模型服务商」中配置 DeepL API Key 后,翻译页即可选用。DeepL 为专用翻译引擎,
            不参与对话与模型列表拉取;术语表对 DeepL 同样生效(译文通常已较稳定,术语注入以
            LLM 类引擎为主)。
          </div>
        </div>
      </div>
    </>
  );

  const storageView = (
    <>
      {dirRestarting && (
        <div className="dialog-mask">
          <div className="dialog">
            <div className="dialog-head">正在重启应用</div>
            <div className="dialog-body">
              <p className="set-hint">数据正在迁移到你选择的新目录,应用即将自动重新启动…</p>
            </div>
          </div>
        </div>
      )}
      <h2>存储位置</h2>
      <p className="set-desc">
        无账号体系,数据只存本机,统一落在下方所示的本地数据目录下(可自定义位置)。
      </p>

      <div className="set-card">
        <div className="set-card-h">当前数据目录</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">目录</label>
            <span
              style={{
                flex: 1,
                fontFamily: 'var(--font-mono)',
                color: 'var(--text-1)',
                wordBreak: 'break-all',
              }}
            >
              {dataDir?.path ?? '读取中…'}
            </span>
          </div>
          <div className="set-row">
            <label className="lbl">引导配置</label>
            <span
              className="set-hint"
              style={{ fontFamily: 'var(--font-mono)', wordBreak: 'break-all' }}
            >
              {dataDir?.config_path ?? '—'}
            </span>
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-primary" onClick={onChangeDataDir} disabled={dirBusy}>
              更改目录
            </button>
            <button className="btn-ghost" onClick={onOpenDataDir}>
              在资源管理器中打开
            </button>
            <span className="sp" />
            {dataDir?.legacy_adopted && (
              <span className="set-hint">检测到历史数据目录,建议更改到自定义位置</span>
            )}
          </div>
          {dataDir?.migration_error && (
            <div className="set-hint" style={{ color: 'var(--err)' }}>
              上次迁移失败:{dataDir.migration_error}
            </div>
          )}
          {dirMsg && (
            <div
              className="set-hint"
              style={{ color: dirMsg.includes('失败') ? 'var(--err)' : 'var(--text-2)' }}
            >
              {dirMsg}
            </div>
          )}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">内部结构</div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <table className="set-table">
            <thead>
              <tr>
                <th>内容</th>
                <th>路径</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>会话与消息</td>
                <td className="mn">data/metaagent.db</td>
              </tr>
              <tr>
                <td>会话记忆</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.md</td>
              </tr>
              <tr>
                <td>记忆快照</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.versions/</td>
              </tr>
              <tr>
                <td>附件</td>
                <td className="mn">data/attachments/&#123;session&#125;/</td>
              </tr>
              <tr>
                <td>生成图片</td>
                <td className="mn">data/images/</td>
              </tr>
              <tr>
                <td>翻译历史</td>
                <td className="mn">data/translations/</td>
              </tr>
              <tr>
                <td>术语表</td>
                <td className="mn">data/glossary.json</td>
              </tr>
              <tr>
                <td>模型卡覆盖</td>
                <td className="mn">data/model_cards.json</td>
              </tr>
              <tr>
                <td>单模型配置</td>
                <td className="mn">data/provider_model_config.json</td>
              </tr>
              <tr>
                <td>服务商模板</td>
                <td className="mn">data/provider_templates.json</td>
              </tr>
              <tr>
                <td>厂家模型缓存</td>
                <td className="mn">data/provider_models.json</td>
              </tr>
              <tr>
                <td>深度研究</td>
                <td className="mn">data/research/&#123;id&#125;/report.md</td>
              </tr>
              <tr>
                <td>密钥(加密)</td>
                <td className="mn">data/secrets/&#123;provider&#125;.bin</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  );

  const aboutView = (
    <>
      <h2>关于</h2>

      {/* 产品标识区:logo 原比例 + 名称 + 版本(读 tauri.conf.json)+ 定位语 */}
      <div className="set-card set-product">
        <img className="set-product-logo" src={logoUrl} alt="MAF Agent" />
        <div className="set-product-info">
          <div className="set-product-name">MAF Agent</div>
          <div className="set-product-ver">
            版本 {appVersion || '1.9.3'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            一款专为学习、调研而创造的智能体框架软件。MAF 旗下完全免费且无广告的 AI
            框架软件,开发者:梧桐吾桐及其团队。
          </div>
        </div>
      </div>

      {/* 条款文档区:十五条,全部展开 */}
      <div className="set-card">
        <div className="set-card-h">
          MAF Agent 使用条款
          <span className="st date">最近更新日期:2026 年 10 月 19 日</span>
        </div>
        <div className="set-card-b set-terms">
          {TERMS.map((t) => (
            <section key={t.no} className="set-term">
              <h3 className="set-term-h">第{CN_NUM[t.no]}条 · {t.name}</h3>
              {t.items.map((it, i) => {
                // 「1.1」这类编号独立成列,正文统一悬挂缩进,便于逐条对照
                const m = /^(\d+\.\d+)\s+([\s\S]*)$/.exec(it.text);
                return (
                  <div key={i} className="set-term-item">
                    <span className="set-term-no">{m?.[1] ?? ''}</span>
                    <div className="set-term-body">
                      <p>{m?.[2] ?? it.text}</p>
                      {it.bullets && (
                        <ul>
                          {it.bullets.map((b, j) => (
                            <li key={j}>{b}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </div>
                );
              })}
            </section>
          ))}
          <p className="set-term-foot">
            本条款为软件使用协议文本。开发者并非专业法律服务机构,如您对条款有疑问,建议在使用前自行咨询专业人士。
          </p>
        </div>
      </div>
    </>
  );

  // ---- 划词提问(迭代 D9)----

  const patchSettings = (patch: Partial<ipc.AppSettings>) =>
    setSettings((s) => (s ? { ...s, ...patch } : s));

  /** 保存:划词开关等 settings。 */
  const onSaveSelection = async () => {
    await onSaveSettings();
  };

  const selectionView = (
    <>
      <h2>划词提问</h2>
      <p className="set-desc">
        在任意应用中选中文本后连按两次 Ctrl+C,主窗口会被唤起并新建会话,选中文字自动填入输入框,按回车即可提问。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          触发开关
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">划词提问</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.selection_ask_enabled ?? false}
              className={`set-switch${settings?.selection_ask_enabled ? ' on' : ''}`}
              onClick={() =>
                patchSettings({ selection_ask_enabled: !settings?.selection_ask_enabled })
              }
            >
              <span className="knob" />
            </button>
            <span className="set-hint">关闭后连按两次 Ctrl+C 不再唤起主窗口</span>
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSelection} disabled={savingSettings}>
              保存划词设置
            </button>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">主窗口 / 托盘</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">最小化到托盘</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.minimize_to_tray ?? false}
              className={`set-switch${settings?.minimize_to_tray ? ' on' : ''}`}
              onClick={() => patchSettings({ minimize_to_tray: !settings?.minimize_to_tray })}
            >
              <span className="knob" />
            </button>
            <span className="set-hint">
              默认关:点 × 退出应用,最小化保留任务栏图标;开启后点 × 与最小化都隐藏到托盘
            </span>
          </div>
        </div>
      </div>
    </>
  );

  // ---- 导航菜单可见性(标题栏 ⋮ 菜单显示哪些功能) ----

  const hiddenNav = settings?.hidden_nav_items ?? [];

  const toggleNavItem = (key: string) =>
    patchSettings({
      hidden_nav_items: hiddenNav.includes(key)
        ? hiddenNav.filter((k) => k !== key)
        : [...hiddenNav, key],
    });

  const navmenuView = (
    <>
      <h2>导航菜单</h2>
      <p className="set-desc">
        控制标题栏 ⋮ 下拉菜单中显示的功能入口。「对话」与「设置」固定保留,其余项可自行决定是否展示;
        隐藏后不影响功能本身,仍可从会话、搜索等入口访问对应内容。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          功能入口
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          {NAV_VISIBILITY.map((it) => {
            const shown = !hiddenNav.includes(it.key);
            return (
              <div className="set-row" key={it.key}>
                <label className="lbl">{it.label}</label>
                <button
                  type="button"
                  role="switch"
                  aria-checked={shown}
                  className={`set-switch${shown ? ' on' : ''}`}
                  onClick={() => toggleNavItem(it.key)}
                >
                  <span className="knob" />
                </button>
                <span className="set-hint">{shown ? '在菜单中显示' : '已隐藏'}</span>
              </div>
            );
          })}
          <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
            对话与设置始终显示,无需设置。
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSettings} disabled={savingSettings}>
              保存界面设置
            </button>
          </div>
        </div>
      </div>
    </>
  );

  // ---- 用量统计 ----

  const DIM_OPTIONS = [
    { value: 'model' as const, label: '按模型' },
    { value: 'provider' as const, label: '按服务商' },
    { value: 'key' as const, label: '按 Key' },
    { value: 'agent' as const, label: '按助手' },
  ];

  const fmtNum = (n: number) => n.toLocaleString('en-US');

  /** 微元(1e-6 元) → 人民币展示;量级越小保留位数越多。 */
  const fmtCost = (micro: number | null | undefined) => {
    if (micro == null) return '—';
    const yuan = micro / 1e6;
    if (yuan === 0) return '¥0';
    if (yuan < 0.01) return `¥${yuan.toFixed(6)}`;
    if (yuan < 1) return `¥${yuan.toFixed(4)}`;
    return `¥${yuan.toFixed(2)}`;
  };

  /** Unix 秒 → MM-DD HH:mm。 */
  const fmtClock = (secs: number) => {
    const d = new Date(secs * 1000);
    const p = (x: number) => String(x).padStart(2, '0');
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };

  const STATUS_LABEL: Record<string, string> = {
    success: '成功',
    failed: '失败',
    rate_limited: '限流',
  };

  const KIND_LABEL: Record<string, string> = {
    stream: '流中断',
    auth: '鉴权失败',
    server: '服务端错误',
    network: '网络错误',
    bad_request: '请求错误',
    rate_limited: '限流',
    not_supported: '不支持',
    parse: '解析失败',
    other: '其他',
  };

  const usageTotals = usage?.totals;
  const usageGroups = usage?.groups ?? [];
  const maxReq = usageGroups.reduce((m, g) => (g.requests > m ? g.requests : m), 0) || 1;
  const failRate =
    usageTotals && usageTotals.requests > 0
      ? `${((usageTotals.failed / usageTotals.requests) * 100).toFixed(1)}%`
      : '0%';
  const avgFirst =
    usageTotals?.avg_first_token_ms != null ? `${usageTotals.avg_first_token_ms} ms` : '—';

  /** 明细行归属的分类键(与后端 summary_on 的 key 表达式一致),用于下钻过滤。 */
  const drillKeyOf = (e: ipc.UsageEntry) => {
    switch (usageDim) {
      case 'provider':
        return e.provider_id;
      case 'key':
        return String(e.slot_id);
      case 'agent':
        return e.agent_id || 'none';
      default:
        return e.model_id;
    }
  };
  const drillLabel = usageDrill
    ? usageGroups.find((g) => g.key === usageDrill)?.name ?? usageDrill
    : null;
  const usageListShown = usageDrill ? usageList.filter((e) => drillKeyOf(e) === usageDrill) : usageList;

  /** 热力图:星期(0=周日)× 小时,值为请求数。 */
  const heatCells = (() => {
    const grid = new Map<string, ipc.UsageHour>();
    for (const h of usageHours) grid.set(`${h.dow}-${h.hour}`, h);
    return grid;
  })();
  const heatMax = usageHours.reduce((m, h) => (h.requests > m ? h.requests : m), 0) || 1;
  const DOW_LABEL = ['日', '一', '二', '三', '四', '五', '六'];

  /** Key 健康表:各 Provider × 槽位的成功率 / 首字延迟。 */
  const healthRows = Object.entries(usageHealth).flatMap(([pid, groups]) =>
    groups.map((g) => ({ ...g, provider_id: pid })),
  );

  const usageView = (
    <>
      <h2>用量统计</h2>
      <p className="set-desc">
        统计本机请求的 Token 用量、失败率与首字延迟,可按模型 / 服务商 / API Key 分类查看。数据仅存本地。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          总览
          <span className="sp" />
          <div className="seg">
            {[7, 30, 90].map((d) => (
              <button key={d} className={usageDays === d ? 'on' : ''} onClick={() => setUsageDays(d)}>
                近 {d} 天
              </button>
            ))}
          </div>
        </div>
        <div className="set-card-b">
          <div className="usage-metrics">
            <div className="usage-metric">
              <div className="v">{fmtNum(usageTotals?.requests ?? 0)}</div>
              <div className="k">总请求数</div>
            </div>
            <div className="usage-metric">
              <div className="v">{ipc.formatTokens(usageTotals?.tokens ?? 0)}</div>
              <div className="k">总 Token 用量</div>
            </div>
            <div className="usage-metric">
              <div className="v">{failRate}</div>
              <div className="k">失败率(限流 {fmtNum(usageTotals?.rate_limited ?? 0)} 次)</div>
            </div>
            <div className="usage-metric">
              <div className="v">{avgFirst}</div>
              <div className="k">平均首字延迟</div>
            </div>
            <div className="usage-metric">
              <div className="v">{fmtCost(usageTotals?.cost_micro)}</div>
              <div className="k">总成本(按定价表估算)</div>
            </div>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          时段热力图
          <span className="sp" />
          <span className="set-hint">星期 × 小时,颜色越深请求越多</span>
        </div>
        <div className="set-card-b">
          {usageHours.length === 0 ? (
            <div className="set-hint">该时间范围内暂无用量数据</div>
          ) : (
            <div className="usage-scroll">
              <div className="usage-heat" style={{ minWidth: 520 }}>
              <div className="usage-heat-corner" />
              {Array.from({ length: 24 }, (_, h) => (
                <div className="usage-heat-hd" key={`h${h}`}>
                  {h % 3 === 0 ? h : ''}
                </div>
              ))}
              {DOW_LABEL.map((dl, dow) => (
                <Fragment key={`d${dow}`}>
                  <div className="usage-heat-dow">周{dl}</div>
                  {Array.from({ length: 24 }, (_, h) => {
                    const cell = heatCells.get(`${dow}-${h}`);
                    const v = cell?.requests ?? 0;
                    const alpha = v === 0 ? 0 : 0.15 + 0.85 * (v / heatMax);
                    return (
                      <div
                        key={`c${dow}-${h}`}
                        className="usage-heat-cell"
                        title={`周${dl} ${h}:00 · ${fmtNum(v)} 次 · ${ipc.formatTokens(cell?.tokens ?? 0)} tokens`}
                        style={v === 0 ? undefined : { background: `rgba(59,130,246,${alpha.toFixed(2)})` }}
                      />
                    );
                  })}
                </Fragment>
              ))}
            </div>
            </div>
          )}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          分类统计
          <span className="sp" />
          <div className="seg">
            {DIM_OPTIONS.map((o) => (
              <button
                key={o.value}
                className={usageDim === o.value ? 'on' : ''}
                onClick={() => setUsageDim(o.value)}
              >
                {o.label}
              </button>
            ))}
          </div>
        </div>
        <div className="set-card-b">
          {usageGroups.length === 0 ? (
            <div className="set-hint">该时间范围内暂无用量数据</div>
          ) : (
            usageGroups.map((g) => (
              <div
                className={`usage-bar-row${usageDrill === g.key ? ' on' : ''}`}
                key={g.key}
                onClick={() => setUsageDrill((d) => (d === g.key ? null : g.key))}
                role="button"
                title="点击下钻:只看该分类的请求明细"
              >
                <span className="nm" title={g.name}>
                  {g.name}
                  {g.cooling && <span className="cool-num">冷却中</span>}
                </span>
                <span className="bar">
                  <i style={{ width: `${Math.max(3, Math.round((g.requests / maxReq) * 100))}%` }} />
                </span>
                <span className="num">
                  {fmtNum(g.requests)} 次 · {ipc.formatTokens(g.tokens)}
                  {g.cost_micro > 0 ? ` · ${fmtCost(g.cost_micro)}` : ''}
                  {g.failed > 0 ? ` · 失败 ${g.failed}` : ''}
                </span>
              </div>
            ))
          )}
        </div>
      </div>

      {healthRows.length > 0 && (
        <div className="set-card">
          <div className="set-card-h">Key 健康</div>
          <div className="set-card-b" style={{ padding: 0 }}>
            <div className="usage-scroll">
              <table className="usage-table">
                <thead>
                  <tr>
                    <th>服务商</th>
                    <th>槽位</th>
                    <th>请求</th>
                    <th>成功</th>
                    <th>失败</th>
                    <th>限流</th>
                    <th>成功率</th>
                    <th>首字</th>
                  </tr>
                </thead>
                <tbody>
                  {healthRows.map((r) => {
                    const ok = r.requests - r.failed;
                    const rate = r.requests > 0 ? `${((ok / r.requests) * 100).toFixed(1)}%` : '—';
                    return (
                      <tr key={`${r.provider_id}-${r.key}`}>
                        <td>{r.provider_id}</td>
                        <td className="mono">
                          #{r.slot_id ?? '—'}
                          {r.slot_id != null ? ` ${r.name}` : ''}
                        </td>
                        <td className="mono">{fmtNum(r.requests)}</td>
                        <td className="mono st-ok">{fmtNum(ok)}</td>
                        <td className={`mono${r.failed > 0 ? ' st-fail' : ''}`}>{fmtNum(r.failed)}</td>
                        <td className={`mono${r.rate_limited > 0 ? ' st-rl' : ''}`}>
                          {fmtNum(r.rate_limited)}
                        </td>
                        <td className="mono">{rate}</td>
                        <td className="mono">
                          {r.avg_first_token_ms != null ? `${r.avg_first_token_ms}ms` : '—'}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      <div className="set-card">
        <div className="set-card-h">
          最近请求明细
          {drillLabel && (
            <>
              <span className="sp" />
              <span className="usage-drill">
                已下钻:{drillLabel}
                <button onClick={() => setUsageDrill(null)}>清除</button>
              </span>
            </>
          )}
        </div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <div className="usage-scroll">
            <table className="usage-table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>模型</th>
                  <th>助手</th>
                  <th>槽位</th>
                  <th>Key</th>
                  <th>状态</th>
                  <th>Token</th>
                  <th>成本</th>
                  <th>首字</th>
                </tr>
              </thead>
              <tbody>
                {usageListShown.map((e) => (
                  <tr key={e.id}>
                    <td className="mono">{fmtClock(e.created_at)}</td>
                    <td>{e.model_id}</td>
                    <td>{e.agent_name ?? '—'}</td>
                    <td className="mono">
                      #{e.slot_id}
                      {e.slot_label ? ` ${e.slot_label}` : ''}
                    </td>
                    <td className="mono">{e.key_masked ?? '—'}</td>
                    <td
                      className={
                        e.status === 'success'
                          ? 'st-ok'
                          : e.status === 'rate_limited'
                            ? 'st-rl'
                            : 'st-fail'
                      }
                    >
                      {STATUS_LABEL[e.status] ?? e.status}
                      {e.error_kind ? ` · ${KIND_LABEL[e.error_kind] ?? e.error_kind}` : ''}
                    </td>
                    <td className="mono">{e.total_tokens != null ? fmtNum(e.total_tokens) : '—'}</td>
                    <td className="mono">{fmtCost(e.cost_micro)}</td>
                    <td className="mono">{e.first_token_ms != null ? `${e.first_token_ms}ms` : '—'}</td>
                  </tr>
                ))}
                {usageListShown.length === 0 && (
                  <tr>
                    <td colSpan={9}>暂无记录</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </>
  );

  const viewByNav: Record<string, ReactElement> = {
    providers: providersView,
    memory: memoryView,
    search: searchView,
    translate: translateView,
    selection: selectionView,
    navmenu: navmenuView,
    usage: usageView,
    storage: storageView,
    about: aboutView,
  };

  return (
    <div className="pg pg-settings">
      {/* 左:设置分区导航(玻璃抽屉,标题栏「面板」按钮释放) */}
      <AsideDrawer>
      <nav className="set-nav drawer">
        <div className="drawer-hint">点标题栏「面板」可开合本栏</div>
        {NAV.map((g) => (
          <div key={g.grp}>
            <div className="grp">{g.grp}</div>
            {g.items.map((it) => {
              const Icon = it.icon;
              return (
                <div
                  key={it.id}
                  className={`it${nav === it.id ? ' on' : ''}`}
                  onClick={() => setNav(it.id)}
                >
                  <Icon size={15} />
                  {it.label}
                </div>
              );
            })}
          </div>
        ))}
      </nav>
      </AsideDrawer>

      <div className="set-content">{viewByNav[nav]}</div>
    </div>
  );
}
