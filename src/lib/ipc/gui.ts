//! GUI 自动化 IPC —— 对应 Rust 侧 commands/gui.rs 与 gui 模块。

import { invoke } from "@tauri-apps/api/core";

/** GUI 自动化策略(持久化在 data/gui-policy.json)。 */
export interface GuiPolicy {
  enabled: boolean;
  /** 允许操作的进程名,如 notepad.exe */
  allowed_apps: string[];
  element_ttl_secs: number;
  max_elements: number;
  max_text_len: number;
}

export interface WindowInfo {
  hwnd: number;
  pid: number;
  process: string;
  title: string;
}

export interface ElementInfo {
  element_id: string;
  hwnd: number;
  process: string;
  name: string;
  automation_id: string;
  control_type: string;
  class_name: string;
  enabled: boolean;
  rect: [number, number, number, number];
}

export const guiGetPolicy = () => invoke<GuiPolicy>("gui_get_policy");
export const guiSavePolicy = (policy: GuiPolicy) => invoke<void>("gui_save_policy", { policy });
export const guiListWindows = () => invoke<WindowInfo[]>("gui_list_windows");

/** 设置页的元素预览(只读;真正执行走模型工具 ui_*)。 */
export const guiFindElements = (
  hwnd: number,
  opts?: { name?: string; automationId?: string; controlType?: string; limit?: number },
) =>
  invoke<ElementInfo[]>("gui_find_elements", {
    hwnd,
    name: opts?.name ?? null,
    automationId: opts?.automationId ?? null,
    controlType: opts?.controlType ?? null,
    limit: opts?.limit ?? null,
  });
