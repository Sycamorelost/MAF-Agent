import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

import type { MemoryDto, VersionItem } from './types';

// ============ memory ============

export const readMemory = (sessionId: string) =>
  invoke<MemoryDto>('read_memory', { sessionId });

export const writeMemory = (sessionId: string, content: string, note?: string) =>
  invoke<void>('write_memory', { sessionId, content, note: note ?? null });

/** 追加文本到 memory.md 指定节(缺省「资料」,节名限 6 个标准节;note=pin_message)。 */
export const appendMemory = (sessionId: string, text: string, section?: string) =>
  invoke<void>('append_memory', { sessionId, text, section: section ?? null });

/** 手动触发一次版本快照,返回该版本的数字 id(可直接用于 rollbackMemory/readMemoryVersion)。 */
export const snapshotMemory = (sessionId: string, note?: string) =>
  invoke<number>('snapshot_memory', { sessionId, note: note ?? null });

export const listMemoryVersions = (sessionId: string) =>
  invoke<VersionItem[]>('list_memory_versions', { sessionId });

export const rollbackMemory = (sessionId: string, versionId: number) =>
  invoke<void>('rollback_memory', { sessionId, versionId });

/** 读取指定历史版本的完整快照内容(D3.3 版本 diff 用)。 */
export const readMemoryVersion = (sessionId: string, versionId: number) =>
  invoke<string>('read_memory_version', { sessionId, versionId });

/** 记忆注入预算状态(D3.5)。 */
export interface BudgetStatus {
  tokens: number;
  limit: number;
  over: boolean;
}

export const memoryBudgetStatus = (sessionId: string) =>
  invoke<BudgetStatus>('memory_budget_status', { sessionId });

export const watchMemory = (sessionId: string) =>
  invoke<void>('watch_memory', { sessionId });

export const unwatchMemory = (sessionId: string) =>
  invoke<void>('unwatch_memory', { sessionId });

export const needsSummarize = (sessionId: string) =>
  invoke<boolean>('needs_summarize', {
    sessionId,
    everyNRounds: null,
    tokenThreshold: null,
  });

export interface SummarizeMemoryInput {
  session_id: string;
  provider_id: string;
  model: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
}

/** 调 LLM 提炼记忆,返回更新后的完整 memory.md(含 frontmatter);不落盘。 */
export const summarizeMemory = (input: SummarizeMemoryInput) =>
  invoke<string>('summarize_memory', { input });

// ============ 应用设置(D3.7 / D9)============

export interface AppSettings {
  /** 提炼专用 provider_id(null = 用当前会话模型) */
  summarize_provider_id: string | null;
  /** 提炼专用模型(null = 用当前会话模型) */
  summarize_model: string | null;
  /** 划词提问:任意应用中连按两次 Ctrl+C 唤起主窗口 */
  selection_ask_enabled: boolean;
  /** 主窗口最小化时隐藏到托盘 */
  minimize_to_tray: boolean;
  /** 导航菜单中隐藏的功能项(对话/设置始终显示) */
  hidden_nav_items: string[];
}

export const getSettings = () => invoke<AppSettings>('get_settings');

/** 保存设置并广播 settings_changed。 */
export const saveSettings = async (settings: AppSettings) => {
  await invoke<void>('save_settings', { settings });
  await emit('settings_changed', settings);
};

// ============ 数据目录(可自定义)============

export interface DataDirInfo {
  /** 当前数据根目录(绝对路径) */
  path: string;
  /** 引导配置文件路径(exe 旁 config.json,不可写时回退 %APPDATA%) */
  config_path: string;
  /** 是否已由用户显式选定过 */
  configured: boolean;
  /** 是否采用了历史数据目录(老版本升级) */
  legacy_adopted: boolean;
  /** 当前目录是否已有数据库文件 */
  has_data: boolean;
  /** 上次迁移失败的原因 */
  migration_error: string | null;
  /** 是否为开发构建(开发期不弹首次启动引导) */
  is_dev: boolean;
}

export interface SetDataDirResult {
  /** 是否需要重启应用才能生效 */
  restart_required: boolean;
  /** 待迁移的旧目录(无则 null) */
  old_dir: string | null;
}

export const getDataDir = () => invoke<DataDirInfo>('get_data_dir');

/** 弹出原生文件夹选择器;用户取消返回 null。 */
export const pickDataDir = () => invoke<string | null>('pick_data_dir');

/** 切换数据目录;migrate=true 时把旧目录数据搬到新目录(下次启动执行)。 */
export const setDataDir = (path: string, migrate: boolean) =>
  invoke<SetDataDirResult>('set_data_dir', { path, migrate });

/** 在资源管理器中打开当前数据目录。 */
export const openDataDir = () => invoke<void>('open_data_dir');

/** 重启应用(目录切换与迁移需要新进程生效)。 */
export const restartApp = () => invoke<void>('restart_app');



