//! 通用设置数据源:应用设置 / 数据目录 / 联网搜索 / 术语表 / 导航可见性。
//!
//! 从 SettingsPage 抽出(§10.2.1),页面只负责展示与 props 组装。

import { useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from '../../lib/ipc';
import { ENGINES, NAV_VISIBILITY } from './constants';
import type { SearchDraft } from './SearchView';

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

  // 加载(顺序与原 SettingsPage 初始化 effect 一致)
  useEffect(() => {
    ipc.getSettings().then((s) => setSettings(s));
    ipc.getDataDir().then(setDataDir).catch(() => {});
    ipc.getSearchConfig().then((c) => {
      setSearchCfg(c);
      setSearchDraft((d) => ({ ...d, engine: c.engine, base_url: c.base_url }));
    });
    ipc.glossaryList().then(setGlossary);
  }, []);

  const engineLabel = ENGINES.find((e) => e.id === searchCfg?.engine)?.name ?? '未配置';
  const engineReady = searchCfg
    ? searchCfg.engine === 'searxng'
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

  const patchSearchDraft = (patch: Partial<SearchDraft>) =>
    setSearchDraft((d) => ({ ...d, ...patch }));

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

  // ---- 划词提问(迭代 D9)----

  const patchSettings = (patch: Partial<ipc.AppSettings>) =>
    setSettings((s) => (s ? { ...s, ...patch } : s));

  /** 保存:划词开关等 settings。 */
  const onSaveSelection = async () => {
    await onSaveSettings();
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
    glossary,
    glossaryDirty,
    glossaryMsg,
    savingGlossary,
    importRef,
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
    mutateGlossary,
    onSaveGlossary,
    onImportGlossary,
    patchSettings,
    onSaveSelection,
    toggleNavItem,
    reorderNavItem,
  };
}
