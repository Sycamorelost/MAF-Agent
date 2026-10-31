/**
 * 专业模式首页(P3 / D7):模式徽标 + 新建任务入口 + 快捷提示 + 最近工作台会话。
 * 欢迎语沿用 home_prompt;任务数据(tasks 表)在 P4 接入,此处先列工作台会话。
 */
import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import {
  IconZap,
  IconGauge,
  IconSettings,
  IconNote,
  IconChart,
  IconFileText,
  IconRefresh,
  IconArrowRight,
} from '../../components/icons';
import { useSessionsStore } from '../../stores/sessions';
import { useProvidersStore } from '../../stores/providers';
import { DEFAULT_HOME_PROMPT } from '../HomePage';
import { resolveMode, useDefaultWorkbench } from '../../lib/workbench';
import { RecentTasks } from './components/RecentTasks';
import { NewTaskBox } from './components/NewTaskBox';
import { StatusBar } from '../../components/StatusBar';
import * as ipc from '../../lib/ipc';

/** 快捷提示:图标 + 文本(ui-design b2)。 */
const QUICK_PROMPTS = [
  { icon: IconNote, text: '整理本周会议纪要' },
  { icon: IconChart, text: '生成四档预设对比表' },
  { icon: IconFileText, text: '把文档转成 PDF' },
  { icon: IconRefresh, text: '复盘这段对话' },
] as const;

export default function WorkbenchHome() {
  const nav = useNavigate();
  const sessions = useSessionsStore((s) => s.sessions);
  const create = useSessionsStore((s) => s.create);
  const setWorkbenchMode = useSessionsStore((s) => s.setWorkbenchMode);
  const setPendingComposer = useSessionsStore((s) => s.setPendingComposer);
  const current = useProvidersStore((s) => s.current);
  const def = useDefaultWorkbench();
  const [text, setText] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [greeting, setGreeting] = useState(DEFAULT_HOME_PROMPT);
  const [presetName, setPresetName] = useState('标准');

  // 欢迎语:读取设置里的自定义文案,并随设置保存实时更新(与 HomePage 同源)
  useEffect(() => {
    let alive = true;
    const apply = (s: ipc.AppSettings) => {
      if (alive) setGreeting(s.home_prompt?.trim() || DEFAULT_HOME_PROMPT);
    };
    ipc.getSettings().then(apply).catch(() => {});
    const un = listen<ipc.AppSettings>('settings_changed', (e) => apply(e.payload));
    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, []);

  // 运行时预设 chip:显示全局默认档位名称(点击跳到预设设置)
  useEffect(() => {
    let alive = true;
    void Promise.all([ipc.listPresets(), ipc.getSettings()])
      .then(([presets, s]) => {
        if (!alive) return;
        const hit = presets.find((p) => p.id === s.default_preset);
        setPresetName(hit?.name ?? '标准');
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  /** 最近工作台会话:生效模式为 work 的会话(覆盖值优先,inherit 跟随全局默认)。 */
  const recent = useMemo(
    () =>
      sessions
        .filter((s) => resolveMode(s.workbench_mode, def ?? 'chat') === 'work')
        .slice(0, 6),
    [sessions, def],
  );

  const onCreate = async () => {
    const v = text.trim();
    if (!v || creating) return;
    setCreating(true);
    setError(null);
    try {
      const title = v.length > 24 ? `${v.slice(0, 24)}…` : v;
      const modelId = current ? `${current.providerId}/${current.model}` : '';
      const id = await create(title, modelId, null);
      await setWorkbenchMode(id, 'work');
      setPendingComposer({ sessionId: id, text: v });
      nav(`/work/${id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setCreating(false);
    }
  };

  return (
    <>
      <div className="workhome">
        <div className="workhome-inner">
          <div className="wh-head">
            <div className="wh-mode">
              <button
                type="button"
                className="chip"
                title="全局默认运行时预设(点击修改)"
                onClick={() => nav('/settings?tab=presets')}
              >
                <IconGauge size={12} />
                运行时预设 · {presetName}
              </button>
              <span className="sp" />
              <button
                type="button"
                className="btn-ghost btn-xs"
                onClick={() => nav('/settings?tab=workbench')}
              >
                <IconSettings size={12} />
                工作台设置
              </button>
            </div>
            <h1>{greeting}</h1>
            <p>
              描述目标,Agent 会拆解计划、调用工具、交付可回滚的产物;过程与依据都保留在检查器里。
            </p>
          </div>

          <NewTaskBox
            text={text}
            presetName={presetName}
            creating={creating}
            onText={setText}
            onCreate={() => void onCreate()}
            onOpenPreset={() => nav('/settings?tab=presets')}
          />
          {error && (
            <div style={{ fontSize: 'var(--font-caption)', color: 'var(--err)' }}>{error}</div>
          )}

          <div>
            <div className="quick-label">
              <IconZap size={12} />
              快捷提示
            </div>
            <div className="quick-row">
              {QUICK_PROMPTS.map(({ icon: Icon, text: q }) => (
                <button key={q} type="button" className="quick" onClick={() => setText(q)}>
                  <Icon size={13} />
                  {q}
                </button>
              ))}
            </div>
          </div>

          <div>
            <div className="sec-head">
              <h3>最近任务</h3>
              <span className="cnt">{recent.length} 个进行中或已完成</span>
              <span className="sp" />
              {recent.length > 0 && (
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  onClick={() => nav(`/work/${recent[0].id}`)}
                >
                  继续最近
                  <IconArrowRight size={12} />
                </button>
              )}
            </div>
            <RecentTasks items={recent} onOpen={(sid) => nav(`/work/${sid}`)} />
          </div>
        </div>
      </div>
      <StatusBar modeLabel="专业 Agent" presetName={presetName} />
    </>
  );
}
