//! 运行时预设 IPC 封装与类型(P5 / D18 / D19,与后端 presets 模块对齐)。

import { invoke } from '@tauri-apps/api/core';

export interface PresetToolFilter {
  include: string[];
  exclude: string[];
}

export interface PresetPrune {
  threshold_chars: number;
  head: number;
  tail: number;
}

export interface PresetContext {
  max_messages: number;
  max_chars: number;
  prune: PresetPrune;
}

export interface PresetMemory {
  /** full / summary / hybrid / none(展示用;实际注入以会话策略链为准) */
  strategy: string;
  max_tokens: number;
}

export interface PresetRetrieval {
  lexical: string;
  vector: boolean;
  rerank: boolean;
  top_k: number;
}

/** 运行时预设(与后端 presets::Preset 对齐)。 */
export interface Preset {
  id: string;
  name: string;
  order: number;
  description: string;
  tools: PresetToolFilter;
  context: PresetContext;
  memory: PresetMemory;
  /** 默认审批建议值(ask/workspace/full);实际审批以会话 permission_mode 为准 */
  approval: string;
  retrieval: PresetRetrieval;
}

/** 生效来源:session = 会话钉住;agent = Agent 模板默认;global = 全局默认。 */
export type PresetSource = 'session' | 'agent' | 'global';

/** 会话预设解析结果(与后端 commands::presets::SessionPresetInfo 对齐)。 */
export interface SessionPresetInfo {
  preset: Preset;
  source: PresetSource;
  pinned_id: string | null;
  pinned_version: number | null;
}

export const listPresets = () => invoke<Preset[]>('list_presets');

export const resolveSessionPreset = (sessionId: string) =>
  invoke<SessionPresetInfo>('resolve_session_preset', { sessionId });

/** 设置会话级预设覆盖:presetId 非空钉住(记录清单版本),null 恢复继承。 */
export const setSessionPreset = (sessionId: string, presetId: string | null) =>
  invoke<SessionPresetInfo>('set_session_preset', { sessionId, presetId });
