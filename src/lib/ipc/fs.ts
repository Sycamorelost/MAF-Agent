import { invoke } from '@tauri-apps/api/core';

/** 工作区(Agent 可读写沙箱)相关 IPC。 */

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
}

export const fsWorkspaceInfo = () =>
  invoke<{ root: string; backups: string }>('fs_workspace_info');

export const fsList = (rel?: string) =>
  invoke<FsEntry[]>('fs_list', { rel: rel ?? null });

export const fsBackupList = () => invoke<FsBackup[]>('fs_backup_list');

export const fsRestore = (id: string) => invoke<string>('fs_restore', { id });

export const fsOpenWorkspace = () => invoke<void>('fs_open_workspace');

export const fsOpenBackups = () => invoke<void>('fs_open_backups');
