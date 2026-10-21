import { invoke, Channel } from '@tauri-apps/api/core';
import type { ResearchParams, StageConfig } from './template';

// ============ research(Phase 2 深度研究)============

export interface SourceItem {
  id: number;
  title: string;
  url: string;
  snippet: string;
  engine: string;
  published: string | null;
}

export type ResearchEvent =
  | { type: 'round_start'; round: number; max_rounds: number; stage?: number }
  | { type: 'thought'; round: number; content: string; stage?: number }
  | { type: 'queries'; round: number; queries: string[]; stage?: number }
  | { type: 'searching'; round: number; query: string; stage?: number }
  | { type: 'sources'; round: number; sources: SourceItem[]; stage?: number }
  | { type: 'observation'; round: number; content: string; stage?: number }
  | { type: 'paused'; round: number; stage?: number }
  | { type: 'resumed'; round: number; stage?: number }
  | { type: 'stage_start'; stage: number; name: string; kind: string }
  | { type: 'stage_done'; stage: number; summary: string; sources_count: number; partial?: boolean }
  | { type: 'stage_skipped'; stage: number; reason: string }
  | { type: 'angle_gap'; stage: number; missing: string[] }
  | { type: 'checkpoint'; stage: number; next: string }
  | { type: 'report_delta'; content: string }
  | { type: 'done'; report: string }
  | { type: 'cancelled' }
  | { type: 'error'; message: string };

export interface ResearchRow {
  id: string;
  topic: string;
  status: 'running' | 'paused' | 'done' | 'error' | 'cancelled';
  rounds: number;
  max_rounds: number;
  model_id: string | null;
  error: string | null;
  created: string;
  updated: string;
  /** 研究模式:legacy / staged */
  mode: string;
  /** 当前/最后执行的阶段序号 */
  current_stage: number;
  blueprint_name: string | null;
  /** 已完成阶段数(M4:落库回写,列表零文件 IO;非 staged 恒 0) */
  stages_done: number;
  /** 启用阶段总数(M4:启动时落库;非 staged 恒 0) */
  stages_total: number;
}

/** 阶段运行期状态(stages.json 快照)。 */
export interface StageState {
  id: string;
  name: string;
  kind: string;
  enabled: boolean;
  /** pending / running / done / skipped */
  status: string;
  summary: string;
  sources_count: number;
  rounds: number;
  /** M2:true = 因失败策略提前中止,摘要为部分结果 */
  partial?: boolean;
}

export interface StagesFile {
  mode: string;
  stages: StageState[];
  updated: string;
  /** H1:完整阶段配置快照(检查点「调整下阶段」的数据源) */
  configs?: StageConfig[];
}

export interface ResearchDetail {
  session: ResearchRow;
  trail: ResearchEvent[];
  report: string | null;
  /** 专项模式阶段快照(legacy 为 null) */
  stages: StagesFile | null;
}

export interface SearchConfigDto {
  engine: string;
  base_url: string;
  has_api_key: boolean;
}

export const getSearchConfig = () => invoke<SearchConfigDto>('get_search_config');

export const saveSearchConfig = (input: {
  engine: string;
  base_url: string;
  api_key?: string | null;
}) => invoke<void>('save_search_config', { input });

export const searchTest = (query: string) => invoke<SourceItem[]>('search_test', { query });

export const researchList = () => invoke<ResearchRow[]>('research_list');

export const researchGet = (id: string) => invoke<ResearchDetail>('research_get', { id });

export const researchDelete = (id: string) => invoke<void>('research_delete', { id });

export const researchCancel = (id: string) => invoke<void>('research_cancel', { id });

export const researchPause = (id: string) => invoke<void>('research_pause', { id });

export const researchResume = (id: string) => invoke<void>('research_resume', { id });

/** 检查点确认继续(专项研究阶段间检查点操作条)。 */
export const researchStageContinue = (id: string) =>
  invoke<void>('research_stage_continue', { id });

/** H1:检查点「调整下阶段」— 写回最新阶段编排,引擎在下一阶段起点热读生效。 */
export const researchUpdateStages = (id: string, stages: StageConfig[]) =>
  invoke<void>('research_update_stages', { id, stages });

export interface ResearchStartInput {
  topic: string;
  provider_id: string;
  model: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
  params?: Partial<ResearchParams>;
  /** 来源研究方案名(方案库应用后随启动落库) */
  blueprint_name?: string | null;
}

/**
 * 启动深度研究。返回 { id, done, cancel }:
 *   - id:研究 id 的 Promise(命令返回后 resolve)
 *   - done:收到 done / cancelled 时 resolve,error 时 reject
 *   - cancel:等 id 就绪后调 research_cancel 终止运行
 */
export function researchStart(
  input: ResearchStartInput,
  onEvent: (ev: ResearchEvent) => void,
): { id: Promise<string>; done: Promise<void>; cancel: () => void } {
  const channel = new Channel<ResearchEvent>();
  let settled = false;
  let resolveDone!: () => void;
  let rejectDone!: (e: unknown) => void;
  const done = new Promise<void>((res, rej) => {
    resolveDone = res;
    rejectDone = rej;
  });

  channel.onmessage = (ev) => {
    onEvent(ev);
    if (ev.type === 'done' || ev.type === 'cancelled') {
      settled = true;
      resolveDone();
    } else if (ev.type === 'error') {
      settled = true;
      rejectDone(new Error(ev.message));
    }
  };

  const id = invoke<string>('research_start', { input, channel });
  id.catch((e) => {
    if (!settled) {
      settled = true;
      rejectDone(e);
    }
  });

  return {
    id,
    done,
    cancel: () => {
      id.then((rid) => researchCancel(rid).catch(() => {})).catch(() => {});
    },
  };
}
