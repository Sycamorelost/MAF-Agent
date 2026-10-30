import { invoke } from '@tauri-apps/api/core';

/** 工作区(相对路径基准目录)相关 IPC。 */

export interface FsEntry {
  name: string;
  rel: string;
  is_dir: boolean;
  bytes: number;
}

export interface FsWriteResult {
  rel: string;
  bytes: number;
  backup_id: string | null;
}

export interface FsBackup {
  id: string;
  rel: string;
  created: string;
  bytes: number;
  existed: boolean;
  /** V28/P8:归属会话 id(旧备份没有 → null) */
  session_id: string | null;
  /** V28/P8:归属文件夹 id(旧备份没有 → null) */
  folder_id: string | null;
}

export const fsWorkspaceInfo = () =>
  invoke<{ root: string; backups: string }>('fs_workspace_info');

export const fsList = (rel?: string) =>
  invoke<FsEntry[]>('fs_list', { rel: rel ?? null });

/** 列出备份。sessionId 为字符串时只返回该会话的备份;null 时返回全部(全局回退)。 */
export const fsBackupList = (sessionId?: string | null) =>
  invoke<FsBackup[]>('fs_backup_list', { sessionId: sessionId ?? null });

export const fsRestore = (id: string) => invoke<string>('fs_restore', { id });

export const fsOpenWorkspace = () => invoke<void>('fs_open_workspace');

/** 弹出系统文件夹选择器,返回选中的目录路径(用于设置默认工作区)。 */
export const fsPickWorkspaceDir = () => invoke<string | null>('fs_pick_workspace_dir');

export const fsOpenBackups = () => invoke<void>('fs_open_backups');

// ============ P6 检查器 ============

/** 读取文本文件(默认上限 20 万字符;offset 分页续读)。 */
export const fsReadText = (path: string, offset?: number, maxChars?: number) =>
  invoke<string>('fs_read_text', {
    path,
    offset: offset ?? null,
    max_chars: maxChars ?? null,
  });

/** 读取备份内容(diff 视图用;仅文本,截断 20 万字符)。 */
export const fsBackupRead = (id: string) => invoke<string>('fs_backup_read', { id });

/** 在系统文件管理器中定位工作区内文件(目录直接打开)。 */
export const fsReveal = (path: string) => invoke<void>('fs_reveal', { path });

/** 用系统默认应用打开工作区内文件。 */
export const fsOpen = (path: string) => invoke<void>('fs_open', { path });
