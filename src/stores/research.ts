import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import { useProvidersStore } from './providers';

/** 当前打开/正在运行的研究。 */
export interface ActiveResearch {
  id: string;
  topic: string;
  status: string;
  trail: ipc.ResearchEvent[];
  report: string;
  sources: ipc.SourceItem[];
  currentRound: number;
  maxRounds: number;
  error: string | null;
  /** 研究模式:legacy / staged */
  mode: string;
  /** 专项模式阶段运行状态(与 stages.json 一致;legacy 为空) */
  stages: ipc.StageState[];
}

interface ResearchState {
  list: ipc.ResearchRow[];
  active: ActiveResearch | null;
  searchCfg: ipc.SearchConfigDto | null;
  templates: ipc.ResearchTemplateDto[];
  /** 方案库(内置 + 用户方案) */
  blueprints: ipc.Blueprint[];
  /** 待应用到配置面板的方案(方案库「应用」后由 ConfigPanel 消费) */
  pendingBlueprint: ipc.Blueprint | null;
  refreshList: () => Promise<void>;
  loadSearchCfg: () => Promise<void>;
  saveSearchCfg: (engine: string, baseUrl: string, apiKey?: string) => Promise<void>;
  start: (
    topic: string,
    params: ipc.ResearchParams,
    modelOverride?: { providerId: string; model: string } | null,
    blueprintName?: string | null,
  ) => Promise<void>;
  cancel: () => void;
  pause: () => void;
  resume: () => void;
  /** 检查点确认继续(专项研究) */
  stageContinue: () => void;
  loadBlueprints: () => Promise<void>;
  saveBlueprint: (name: string, desc: string, params: ipc.ResearchParams) => Promise<void>;
  removeBlueprint: (name: string) => Promise<void>;
  /** 方案库「应用」:暂存方案,配置面板消费后覆盖当前配置 */
  applyBlueprint: (bp: ipc.Blueprint) => void;
  /** 配置面板消费暂存方案(取出并清空) */
  consumeBlueprint: () => ipc.Blueprint | null;
  loadTemplates: () => Promise<void>;
  saveTemplate: (key: string, content: string) => Promise<void>;
  resetTemplate: (key: string) => Promise<void>;
  openDetail: (id: string) => Promise<void>;
  closeDetail: () => void;
  remove: (id: string) => Promise<void>;
}

// report_delta 30ms 去抖缓冲(仿 messages.ts 流式去抖)
const FLUSH_MS = 30;
let reportBuf = '';
let reportTimer: ReturnType<typeof setTimeout> | undefined;
let cancelFn: (() => void) | null = null;
// active 尚未建立时暂存到达的事件
let pending: ipc.ResearchEvent[] | null = null;
/**
 * 运行代际令牌:每次 start 递增。研究 A 运行中再启动 B 时,A 的迟到事件
 * (channel 虽隔离,但 applyEvent/reportBuf 为模块级单例)凭旧 token 被丢弃,
 * 不得污染 B 的 trail/report/状态(M-3)。
 */
let runToken = 0;

type Set = (fn: (s: ResearchState) => Partial<ResearchState>) => void;

function flushReport(set: Set) {
  const text = reportBuf;
  reportBuf = '';
  if (!text) return;
  set((s) => (s.active ? { active: { ...s.active, report: s.active.report + text } } : {}));
}

function applyEvent(ev: ipc.ResearchEvent, set: Set) {
  switch (ev.type) {
    case 'round_start':
      set((s) =>
        s.active
          ? {
              active: {
                ...s.active,
                currentRound: ev.round,
                maxRounds: ev.max_rounds,
                trail: [...s.active.trail, ev],
              },
            }
          : {},
      );
      break;
    case 'thought':
    case 'queries':
    case 'searching':
    case 'observation':
      set((s) => (s.active ? { active: { ...s.active, trail: [...s.active.trail, ev] } } : {}));
      break;
    case 'sources': {
      set((s) => {
        if (!s.active) return {};
        const known = new Set(s.active.sources.map((x) => x.id));
        const added = ev.sources.filter((x) => !known.has(x.id));
        return {
          active: {
            ...s.active,
            trail: [...s.active.trail, ev],
            sources: [...s.active.sources, ...added],
          },
        };
      });
      break;
    }
    case 'stage_start':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1 ? { ...st, status: 'running' } : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'stage_done':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1
            ? {
                ...st,
                status: 'done',
                summary: ev.summary,
                sources_count: ev.sources_count,
                partial: ev.partial ?? false,
              }
            : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'stage_skipped':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1 ? { ...st, status: 'skipped' } : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'angle_gap':
      set((s) => (s.active ? { active: { ...s.active, trail: [...s.active.trail, ev] } } : {}));
      break;
    case 'checkpoint':
      // 检查点:引擎随即挂起( Paused 事件随后到达),这里直接置 paused 让操作条即时出现
      set((s) =>
        s.active ? { active: { ...s.active, status: 'paused', trail: [...s.active.trail, ev] } } : {},
      );
      break;
    case 'report_delta':
      reportBuf += ev.content;
      if (!reportTimer) {
        reportTimer = setTimeout(() => {
          reportTimer = undefined;
          flushReport(set);
        }, FLUSH_MS);
      }
      break;
    case 'paused':
      set((s) => (s.active ? { active: { ...s.active, status: 'paused' } } : {}));
      break;
    case 'resumed':
      set((s) => (s.active ? { active: { ...s.active, status: 'running' } } : {}));
      break;
    case 'done':
      if (reportTimer) {
        clearTimeout(reportTimer);
        reportTimer = undefined;
      }
      reportBuf = '';
      set((s) =>
        s.active
          ? { active: { ...s.active, status: 'done', report: ev.report || s.active.report } }
          : {},
      );
      break;
    case 'cancelled':
      if (reportTimer) {
        clearTimeout(reportTimer);
        reportTimer = undefined;
      }
      flushReport(set);
      set((s) => (s.active ? { active: { ...s.active, status: 'cancelled' } } : {}));
      break;
    case 'error':
      if (reportTimer) {
        clearTimeout(reportTimer);
        reportTimer = undefined;
      }
      flushReport(set);
      set((s) =>
        s.active ? { active: { ...s.active, status: 'error', error: ev.message } } : {},
      );
      break;
  }
}

/** 从回放 trail 重组 active(打开历史研究时用)。 */
function rebuildActive(detail: ipc.ResearchDetail): ActiveResearch {
  const sources: ipc.SourceItem[] = [];
  let currentRound = 0;
  for (const ev of detail.trail) {
    if (ev.type === 'round_start') currentRound = ev.round;
    else if (ev.type === 'sources') {
      const known = new Set(sources.map((x) => x.id));
      for (const src of ev.sources) if (!known.has(src.id)) sources.push(src);
    }
  }
  return {
    id: detail.session.id,
    topic: detail.session.topic,
    status: detail.session.status,
    trail: detail.trail,
    report: detail.report ?? '',
    sources,
    currentRound,
    maxRounds: detail.session.max_rounds,
    error: detail.session.error,
    mode: detail.session.mode,
    stages: detail.stages?.stages ?? [],
  };
}

export const useResearchStore = create<ResearchState>((set, get) => ({
  list: [],
  active: null,
  searchCfg: null,
  templates: [],
  blueprints: [],
  pendingBlueprint: null,

  refreshList: async () => {
    const list = await ipc.researchList();
    set({ list });
  },

  loadSearchCfg: async () => {
    const searchCfg = await ipc.getSearchConfig();
    set({ searchCfg });
  },

  saveSearchCfg: async (engine, baseUrl, apiKey) => {
    await ipc.saveSearchConfig({ engine, base_url: baseUrl, api_key: apiKey ?? null });
    await get().loadSearchCfg();
  },

  loadBlueprints: async () => {
    const blueprints = await ipc.blueprintList();
    set({ blueprints });
  },

  saveBlueprint: async (name, desc, params) => {
    const blueprints = await ipc.blueprintSave(name, desc, params);
    set({ blueprints });
  },

  removeBlueprint: async (name) => {
    const blueprints = await ipc.blueprintDelete(name);
    set({ blueprints });
  },

  applyBlueprint: (bp) => set({ pendingBlueprint: bp }),

  consumeBlueprint: () => {
    const bp = get().pendingBlueprint;
    if (bp) set({ pendingBlueprint: null });
    return bp;
  },

  loadTemplates: async () => {
    const templates = await ipc.researchTemplateList();
    set({ templates });
  },

  saveTemplate: async (key, content) => {
    const templates = await ipc.researchTemplateSave(key, content);
    set({ templates });
  },

  resetTemplate: async (key) => {
    const templates = await ipc.researchTemplateReset(key);
    set({ templates });
  },

  start: async (topic, params, modelOverride, blueprintName) => {
    const ps = useProvidersStore.getState();
    // 执行模型:默认跟随当前全局选择,可被配置栏覆盖
    const eff = modelOverride ?? ps.current;
    if (!eff) throw new Error('请先在右上角选择模型');
    if (!ps.hasKey[eff.providerId]) throw new Error('当前 Provider 未配置 API Key');
    const customTpl =
      eff.providerId === 'custom' ? ps.templates.find((t) => t.id === 'custom') : null;

    // M-3:启动新研究前先终止上一运行(后端停计费、前端作废旧回调),杜绝串扰
    if (reportTimer) {
      clearTimeout(reportTimer);
      reportTimer = undefined;
    }
    reportBuf = '';
    cancelFn?.();
    cancelFn = null;
    const myToken = ++runToken;

    pending = [];
    const handle = ipc.researchStart(
      {
        topic,
        provider_id: eff.providerId,
        model: eff.model,
        custom_base_url: customTpl?.base_url ?? null,
        custom_display_name: customTpl?.display_name ?? null,
        params,
        blueprint_name: blueprintName ?? null,
      },
      (ev) => {
        // 已有更新的研究启动 → 旧 run 的任何事件(含终态)一律丢弃
        if (myToken !== runToken) return;
        if (pending) pending.push(ev);
        else applyEvent(ev, set);
      },
    );
    cancelFn = handle.cancel;

    const id = await handle.id; // 早失败(未配搜索引擎/Key)在这里抛出
    // 早失败后若用户已发起更新的研究,本次失败结果不再覆盖界面
    if (myToken !== runToken) return;
    set({
      active: {
        id,
        topic,
        status: 'running',
        trail: [],
        report: '',
        sources: [],
        currentRound: 0,
        maxRounds: params.max_rounds,
        error: null,
        mode: params.mode,
        stages:
          params.mode === 'staged'
            ? params.stages.map((st) => ({
                id: st.id,
                name: st.name,
                kind: st.kind,
                enabled: st.enabled,
                status: st.enabled ? 'pending' : 'skipped',
                summary: '',
                sources_count: 0,
                rounds: 0,
              }))
            : [],
      },
    });
    // 放出暂存的早到事件
    const early = pending ?? [];
    pending = null;
    for (const ev of early) applyEvent(ev, set);

    void get().refreshList();
    handle.done
      .catch(() => {})
      .finally(() => {
        // 仅当没有更新的研究启动时才清空句柄,避免旧 run 的收尾杀掉 B 的 cancelFn
        if (myToken === runToken) cancelFn = null;
        void get().refreshList();
      });
  },

  cancel: () => {
    cancelFn?.();
  },

  pause: () => {
    const id = get().active?.id;
    if (id) void ipc.researchPause(id).catch(() => {});
  },

  resume: () => {
    const id = get().active?.id;
    if (id) void ipc.researchResume(id).catch(() => {});
  },

  stageContinue: () => {
    const id = get().active?.id;
    if (id) void ipc.researchStageContinue(id).catch(() => {});
  },

  openDetail: async (id) => {
    // 若正好在看正在运行的 active,不覆盖(事件仍在流入)
    const cur = get().active;
    if (cur && cur.id === id && cur.status === 'running') return;
    const detail = await ipc.researchGet(id);
    set({ active: rebuildActive(detail) });
  },

  closeDetail: () => set({ active: null }),

  remove: async (id) => {
    await ipc.researchDelete(id);
    if (get().active?.id === id) set({ active: null });
    await get().refreshList();
  },
}));
