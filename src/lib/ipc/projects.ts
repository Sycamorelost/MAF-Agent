//! 项目 Project CRUD IPC(P8.1 项目化升级)。
//!
//! 参考 Cursor Projects 设计:
//! - Project = 用户选本地磁盘路径 + 名字 + 可选模型覆盖 + 可选共享上下文
//! - 一个项目下可以挂多个任务会话
//! - 前端先调 fsPickWorkspaceDir(系统原生目录选择器)拿到路径,再调 create

import { invoke } from '@tauri-apps/api/core';

/** 项目行(与后端 ProjectRow 对齐)。 */
export interface ProjectRow {
  id: string;
  name: string;
  path: string;
  model_override: string | null;
  shared_context: string | null;
  sort: number;
  created: string;
  updated: string;
}

export function listProjects(): Promise<ProjectRow[]> {
  return invoke<ProjectRow[]>('project_list');
}

export function createProject(name: string, path: string): Promise<ProjectRow> {
  return invoke<ProjectRow>('project_create', { name, path });
}

export function renameProject(id: string, name: string): Promise<void> {
  return invoke<void>('project_rename', { id, name });
}

export function updateProjectPath(id: string, path: string): Promise<void> {
  return invoke<void>('project_update_path', { id, path });
}

export function updateProjectModel(id: string, model: string | null): Promise<void> {
  return invoke<void>('project_update_model', { id, model });
}

export function updateProjectContext(id: string, ctx: string | null): Promise<void> {
  return invoke<void>('project_update_context', { id, ctx });
}

export function deleteProject(id: string): Promise<void> {
  return invoke<void>('project_delete', { id });
}

export function updateProjectSort(id: string, sort: number): Promise<void> {
  return invoke<void>('project_update_sort', { id, sort });
}

export function setSessionProject(
  sessionId: string,
  projectId: string | null,
): Promise<void> {
  return invoke<void>('session_set_project', {
    sessionId,
    projectId: projectId,
  });
}
