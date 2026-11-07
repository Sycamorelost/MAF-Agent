//! 通用设置数据源:应用设置 / 数据目录 / 联网搜索 / 导航可见性。
//!
//! 从 SettingsPage 抽出(§10.2.1),页面只负责展示与 props 组装。

import { useEffect, useMemo, useState } from 'react';
import * as ipc from '../../lib/ipc';
import { mcpLoadConfig } from '../../lib/ipc/mcp';
import { ENGINES, NAV_VISIBILITY } from './constants';
import type { SearchDraft } from './SearchView';
import { uiConfirm } from '../../components/confirmService';

export function useGeneralSettings() {
  const [settings, setSettings] = useState<ipc.AppSettings | null>(null);
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsSaved, setSettingsSaved] = useState(false);

  // 数据目录(可自定义)
  const [dataDir, setDataDir] = useState<ipc.DataDirInfo | null>(null);
  const [dirMsg, setDirMsg] = useState<string | null>(null);
  const [dirBusy, setDirBusy] = useState(false);
  const [dirRestarting, setDirRestarting] = useState(false);

  const [searchCfg, setSearchCfg] = useState<ipc.SearchConfigDto | null>(null);
  const [searchDraft, setSearchDraft] = useState({
    mode: 'mcp' as ipc.SearchMode,
    engine: 'searxng',
    base_url: '',
    api_key: '',
    mcp_server: '',
    mcp_tool: '',
  });
  const [savingSearch, setSavingSearch] = useState(false);
  const [searchTestStatus, setSearchTestStatus] = useState<string | null>(null);
  const [searchTesting, setSearchTesting] = useState(false);
  /** MCP 调用方式的下拉候选:已有服务名(工具名由后端 get_search_config 直接返回) */
  const [mcpServerNames, setMcpServerNames] = useState<string[]>([]);

  // 加载(顺序与原 SettingsPage 初始化 effect 一致)
  useEffect(() => {
    ipc.getSettings().then((s) => setSettings(s));
    ipc.getDataDir().then(setDataDir).catch(() => {});
    ipc.getSearchConfig().then((c) => {
      setSearchCfg(c);
      setSearchDraft((d) => ({
        ...d,
        mode: c.mode,
        engine: c.engine,
        base_url: c.base_url,
        mcp_server: c.mcp_server,
        mcp_tool: c.mcp_tool,
      }));
    });
  }, []);

  // MCP 调用方式:服务下拉候选(已保存的 MCP server);工具名由后端 get_search_config 实时返回,无需前端单独探测
  useEffect(() => {
    mcpLoadConfig()
      .then((c) => setMcpServerNames(c.servers.map((s) => s.name)))
      .catch(() => setMcpServerNames([]));
  }, []);

  // 当前端改选 MCP 服务时,重新拉一次完整配置(让后端用新服务探测工具名)
  useEffect(() => {
    if (searchDraft.mode !== 'mcp') return;
    ipc.getSearchConfig().then(setSearchCfg).catch(() => {});
  }, [searchDraft.mode, searchDraft.mcp_server]);

  const engineLabel = searchCfg
    ? searchCfg.mode === 'mcp'
      ? `MCP · ${searchCfg.mcp_server_display || searchCfg.mcp_server || '联网搜索'}`
      : (ENGINES.find((e) => e.id === searchCfg.engine)?.name ?? '未配置')
    : '未配置';
  const engineReady = searchCfg
    ? searchCfg.mode === 'mcp'
      ? searchCfg.mcp_ready && (!searchCfg.mcp_needs_key || searchCfg.has_api_key)
      : searchCfg.engine === 'searxng'
        ? !!searchCfg.base_url
        : searchCfg.has_api_key
    : false;

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
      if (
        !(await uiConfirm({
          title: '更改数据目录',
          message: `确定把数据目录更改为:\n${p}${oldTip}\n\n应用将自动重启以完成迁移。`,
        }))
      )
        return;
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
        mode: searchDraft.mode,
        engine: searchDraft.engine,
        base_url: searchDraft.base_url,
        api_key: searchDraft.api_key || null,
        mcp_server: searchDraft.mcp_server,
        mcp_tool: searchDraft.mcp_tool,
      });
      const c = await ipc.getSearchConfig();
      setSearchCfg(c);
      setSearchDraft((d) => ({
        ...d,
        mode: c.mode,
        base_url: c.base_url,
        api_key: '',
        mcp_server: c.mcp_server,
        mcp_tool: c.mcp_tool,
      }));
    } catch (e) {
      setSearchTestStatus(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingSearch(false);
    }
  };

  const onTestSearch = async () => {
    setSearchTesting(true);
    setSearchTestStatus('测试中…');
    const t0 = performance.now();
    const modeLabel = searchDraft.mode === 'mcp' ? 'MCP' : 'API';
    try {
      // 按当前草稿测(未保存也能验证「切换」是否真的生效)
      const res = await ipc.searchTest('myAgent', {
        mode: searchDraft.mode,
        engine: searchDraft.engine,
        base_url: searchDraft.base_url,
        mcp_server: searchDraft.mcp_server,
        mcp_tool: searchDraft.mcp_tool,
      });
      const ms = Math.round(performance.now() - t0);
      if (res.length === 0) {
        setSearchTestStatus(`${modeLabel} 调用成功,但未返回结果(可换关键词)· ${ms}ms`);
      } else {
        const first = res[0];
        setSearchTestStatus(
          `${modeLabel} 调用成功,返回 ${res.length} 条 · ${ms}ms;示例:${first.title || first.url}`,
        );
      }
    } catch (e) {
      const ms = Math.round(performance.now() - t0);
      setSearchTestStatus(
        `${modeLabel} 调用失败(${ms}ms):${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setSearchTesting(false);
    }
  };

  const patchSearchDraft = (patch: Partial<SearchDraft>) =>
    setSearchDraft((d) => ({ ...d, ...patch }));

  // ---- 设置项局部编辑 + 即时写盘 ----

  const patchSettings = (patch: Partial<ipc.AppSettings>) =>
    setSettings((s) => (s ? { ...s, ...patch } : s));

  /** 局部设置即时写盘(附件等卡片的小保存按钮用);失败向上抛出以便展示。 */
  const saveSettingsPatch = async (patch: Partial<ipc.AppSettings>) => {
    if (!settings) throw new Error('设置尚未加载');
    const next = { ...settings, ...patch };
    await ipc.saveSettings(next);
    setSettings(next);
    setSettingsSaved(true);
  };

  // ---- 导航菜单可见性 + 顺序(标题栏 ⋮ 菜单)----
  // 两者都即时写盘:saveSettings 会广播 settings_changed,导航菜单无需刷新即可重排。

  const hiddenNav = settings?.hidden_nav_items ?? [];
  const navOrder = settings?.nav_order ?? [];

  /** 有效顺序:已保存顺序在前,未列出的按内置顺序补在末尾(稳定排序)。 */
  const navMenuOrder = useMemo(() => {
    const rank = new Map(navOrder.map((k, i) => [k, i]));
    return [...NAV_VISIBILITY].sort(
      (a, b) =>
        (rank.get(a.key) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.key) ?? Number.MAX_SAFE_INTEGER),
    );
  }, [navOrder]);

  /** 即时保存:先更新本地视图,再写盘广播(失败保留本地值,不阻塞交互)。 */
  const persistSettings = async (next: ipc.AppSettings) => {
    setSettings(next);
    setSettingsSaved(false);
    try {
      await ipc.saveSettings(next);
      setSettingsSaved(true);
    } catch {
      /* 写盘失败时保持本地乐观值,用户可再点一次保存 */
    }
  };

  const toggleNavItem = (key: string) => {
    if (!settings) return;
    const hidden = settings.hidden_nav_items.includes(key)
      ? settings.hidden_nav_items.filter((k) => k !== key)
      : [...settings.hidden_nav_items, key];
    void persistSettings({ ...settings, hidden_nav_items: hidden });
  };

  /** 关闭主窗口时隐藏到托盘(即时保存并广播,与导航开关同一套机制)。 */
  const toggleMinimizeToTray = () => {
    if (!settings) return;
    void persistSettings({ ...settings, minimize_to_tray: !settings.minimize_to_tray });
  };

  /** 设置全局默认工作台模式(D16),即时保存并广播 settings_changed。 */
  const setDefaultWorkbench = (mode: 'chat' | 'work') => {
    if (!settings) return;
    void persistSettings({ ...settings, default_workbench: mode });
  };

  /** 设置全局默认运行时预设(D19),即时保存;新建会话钉住该档,已有会话不变。 */
  const setDefaultPreset = (presetId: string) => {
    if (!settings) return;
    void persistSettings({ ...settings, default_preset: presetId });
  };

  /** 拖拽排序:把 fromKey 移动到 toKey 所在的位置(前后两个方向都支持)。 */
  const reorderNavItem = (fromKey: string, toKey: string) => {
    if (!settings) return;
    const keys = navMenuOrder.map((i) => i.key);
    const from = keys.indexOf(fromKey);
    const to = keys.indexOf(toKey);
    if (from < 0 || to < 0 || from === to) return;
    keys.splice(to, 0, keys.splice(from, 1)[0]);
    void persistSettings({ ...settings, nav_order: keys });
  };

  return {
    settings,
    savingSettings,
    settingsSaved,
    dataDir,
    dirMsg,
    dirBusy,
    dirRestarting,
    searchCfg,
    searchDraft,
    savingSearch,
    searchTestStatus,
    searchTesting,
    mcpServerNames,
    engineLabel,
    engineReady,
    hiddenNav,
    navOrder,
    navMenuOrder,
    onSaveSettings,
    onChangeDataDir,
    onOpenDataDir,
    onSaveSearch,
    onTestSearch,
    patchSearchDraft,
    patchSettings,
    saveSettingsPatch,
    toggleNavItem,
    reorderNavItem,
    toggleMinimizeToTray,
    setDefaultWorkbench,
    setDefaultPreset,
  };
}
