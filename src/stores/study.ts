// 学习规划板块(P0)状态 —— 事件流/运行令牌模式与研究 store 同构(简化版)。

import { create } from 'zustand';

import * as ipc from '../lib/ipc/study';
import type { PlanPayload, PlanRow, StudyEvent, StudyIntent } from '../lib/ipc/study';
import { useProvidersStore } from './providers';

interface ActiveStudy {
  id: string;
  topic: string;
  /** generating(运行中) / done(已生成载荷) / error / cancelled */
  status: 'generating' | 'done' | 'error' | 'cancelled';
  trail: StudyEvent[];
  payload: PlanPayload | null;
  error: string | null;
}

interface StudyState {
  list: PlanRow[];
  listLoaded: boolean;
  active: ActiveStudy | null;
  /** preflight 澄清结果(questions 非空 → 面板内联追问) */
  intent: StudyIntent | null;
  starting: boolean;

  refreshList: () => Promise<void>;
  preflight: (
    topic: string,
    lang: string,
    hints: { level: string | null; dailyMinutes: number | null; deadline: string | null },
  ) => Promise<void>;
  clearIntent: () => void;
  start: (args: {
    topic: string;
    intentJson: string | null;
    dailyMinutes: number | null;
    deadline: string | null;
    lang: string;
    modelOverride?: { providerId: string; model: string } | null;
  }) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  cancelRun: () => Promise<void>;
  openPlan: (id: string) => Promise<void>;
  closePlan: () => void;
  remove: (id: string) => Promise<void>;
}

let runToken = 0;

/** 取当前(或指定覆盖)模型的生成入参;未选模型/未配 Key 抛错。P0 启动与 P1 学习/测验共用。 */
export function resolveProvider(
  modelOverride?: { providerId: string; model: string } | null,
): {
  provider_id: string;
  model: string;
  custom_base_url: string | null;
  custom_display_name: string | null;
} {
  const ps = useProvidersStore.getState();
  const eff = modelOverride ?? ps.current;
  if (!eff) throw new Error('请先在右上角选择模型');
  if (!ps.hasKey[eff.providerId]) throw new Error('当前 Provider 未配置 API Key');
  const customTpl =
    eff.providerId === 'custom' ? ps.templates.find((t) => t.id === 'custom') : null;
  return {
    provider_id: eff.providerId,
    model: eff.model,
    custom_base_url: customTpl?.base_url ?? null,
    custom_display_name: customTpl?.display_name ?? null,
  };
}

function applyEvent(ev: StudyEvent, get: () => StudyState, set: (p: Partial<StudyState>) => void) {
  const cur = get().active;
  if (!cur) return;
  const trail = [...cur.trail, ev];
  switch (ev.type) {
    case 'done':
      set({
        active: { ...cur, status: 'done', payload: ev.plan ?? cur.payload, trail },
      });
      break;
    case 'cancelled':
      set({ active: { ...cur, status: 'cancelled', trail } });
      break;
    case 'error':
      set({ active: { ...cur, status: 'error', error: ev.message, trail } });
      break;
    default:
      set({ active: { ...cur, trail } });
  }
}

export const useStudyStore = create<StudyState>((set, get) => ({
  list: [],
  listLoaded: false,
  active: null,
  intent: null,
  starting: false,

  refreshList: async () => {
    try {
      const list = await ipc.studyList();
      set({ list, listLoaded: true });
    } catch {
      set({ listLoaded: true });
    }
  },

  preflight: async (topic, lang, hints) => {
    const prov = resolveProvider();
    const intent = await ipc.studyPreflight({
      topic,
      lang,
      ...prov,
      level: hints.level || null,
      daily_minutes: hints.dailyMinutes || null,
      deadline: hints.deadline || null,
    });
    set({ intent });
  },

  clearIntent: () => set({ intent: null }),

  start: async ({ topic, intentJson, dailyMinutes, deadline, lang, modelOverride }) => {
    const prov = resolveProvider(modelOverride);

    const myToken = ++runToken;
    set({
      intent: null,
      starting: false,
      active: { id: '', topic, status: 'generating', trail: [], payload: null, error: null },
    });

    const handle = ipc.studyStart(
      {
        topic,
        intent_json: intentJson,
        daily_minutes: dailyMinutes,
        deadline,
        lang,
        ...prov,
      },
      (ev) => {
        if (myToken !== runToken) return;
        applyEvent(ev, get, (p) => set(p));
      },
    );

    try {
      const id = await handle;
      if (myToken !== runToken) return;
      set((s) => (s.active ? { active: { ...s.active, id } } : s));
      void get().refreshList();
    } catch (e) {
      if (myToken !== runToken) return;
      set({
        active: {
          id: '',
          topic,
          status: 'error',
          trail: [],
          payload: null,
          error: String(e),
        },
      });
    }
  },

  pause: async () => {
    const id = get().active?.id;
    if (id) await ipc.studyPause(id);
  },

  resume: async () => {
    const id = get().active?.id;
    if (id) await ipc.studyResume(id);
  },

  cancelRun: async () => {
    const id = get().active?.id;
    if (id) await ipc.studyCancel(id);
  },

  openPlan: async (id) => {
    const d = await ipc.studyGet(id);
    const status: ActiveStudy['status'] =
      d.payload != null
        ? 'done'
        : d.plan.status === 'error'
          ? 'error'
          : d.plan.status === 'cancelled'
            ? 'cancelled'
            : 'generating';
    set({
      active: {
        id,
        topic: d.plan.topic,
        status,
        trail: d.trail,
        payload: d.payload,
        error: d.plan.error ?? null,
      },
    });
  },

  closePlan: () => {
    runToken++;
    set({ active: null });
  },

  remove: async (id) => {
    await ipc.studyDelete(id);
    if (get().active?.id === id) {
      runToken++;
      set({ active: null });
    }
    await get().refreshList();
  },
}));
