import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import AsideDrawer from '../components/AsideDrawer';
import { MemoMarkdown } from '../components/MemoMarkdown';
import {
  IconArrowLeft,
  IconCheck,
  IconDownload,
  IconGraduation,
  IconPause,
  IconPlay,
  IconResearch,
  IconStop,
  IconTrash,
} from '../components/icons';
import { useStudyStore, resolveProvider } from '../stores/study';
import { useProvidersStore } from '../stores/providers';
import type {
  NodeState,
  PlanPayload,
  PlanRow,
  QuizResult,
  StudyEvent,
  StudyIntent,
  StudyQuiz,
  StudyStats,
  StudyUnit,
  TodayPlan,
} from '../lib/ipc/study';
import * as studyIpc from '../lib/ipc/study';
import { EXPORT_TARGET_EXISTS, pickSavePath } from '../lib/ipc/research';
import { fmtDate, safeExportName } from './research/helpers';

/** 研究页草稿接手约定(见 ResearchPage 挂载逻辑) */
const RESEARCH_DRAFT_KEY = 'metaagent.research.draft';

const STATUS_LABEL: Record<string, string> = {
  generating: '生成中',
  active: '进行中',
  done: '已完成',
  archived: '已归档',
  error: '失败',
  cancelled: '已终止',
};

const LEVEL_LABEL: Record<string, string> = {
  zero: '零基础',
  beginner: '初学者',
  intermediate: '中级',
  advanced: '高级',
};

const levelText = (l: string) => LEVEL_LABEL[l] ?? '未指定';

/** 时间线事件 → 节点(同研究页 buildTimeline 思路:start 开节点,note/retry 追加内容) */
function buildStudyTimeline(trail: StudyEvent[], paused: boolean) {
  const nodes: { key: number; step: string; title: string; state: 'doing' | 'ok'; lines: string[] }[] =
    [];
  const findStep = (step: string) => nodes.find((n) => n.step === step && n.state === 'doing');
  for (const ev of trail) {
    switch (ev.type) {
      case 'step_start': {
        const prev = findStep(ev.step);
        if (prev) prev.state = 'ok';
        nodes.push({ key: nodes.length, step: ev.step, title: ev.label, state: 'doing', lines: [] });
        break;
      }
      case 'step_note': {
        const n = findStep(ev.step);
        if (n) n.lines.push(ev.content);
        break;
      }
      case 'retry': {
        const n = findStep(ev.step);
        if (n) n.lines.push(`第 ${ev.attempt} 次重试:${ev.reason}`);
        break;
      }
      case 'paused': {
        const n = nodes[nodes.length - 1];
        if (n) n.lines.push('已暂停');
        break;
      }
      case 'resumed': {
        const n = nodes[nodes.length - 1];
        if (n) n.lines.push('已继续');
        break;
      }
      case 'done': {
        const n = nodes[nodes.length - 1];
        if (n) n.state = 'ok';
        nodes.push({ key: nodes.length, step: 'done', title: '计划就绪', state: 'ok', lines: [] });
        break;
      }
      case 'cancelled':
        nodes.push({
          key: nodes.length,
          step: 'cancelled',
          title: '已终止',
          state: 'ok',
          lines: [],
        });
        break;
      case 'error':
        nodes.push({
          key: nodes.length,
          step: 'error',
          title: '出错了',
          state: 'ok',
          lines: [ev.message],
        });
        break;
    }
  }
  const last = nodes[nodes.length - 1];
  if (last && last.state === 'doing') last.lines.push(paused ? '已暂停' : '运行中…');
  return nodes;
}

/* ---------------- 左抽屉:规划面板 ---------------- */

function PlanPanel() {
  const { intent, starting, preflight, clearIntent, start } = useStudyStore();
  const current = useProvidersStore((s) => s.current);
  const templatesProviders = useProvidersStore((s) => s.templates);
  const hasKey = useProvidersStore((s) => s.hasKey);

  const [topic, setTopic] = useState('');
  const [level, setLevel] = useState('');
  const [dailyMinutes, setDailyMinutes] = useState(30);
  const [deadline, setDeadline] = useState('');
  const [lang, setLang] = useState('zh');
  const [modelSel, setModelSel] = useState('');
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const modelOptions = useMemo(() => {
    const out: { value: string; label: string }[] = [];
    for (const t of templatesProviders) {
      if (!hasKey[t.id]) continue;
      for (const m of t.models) {
        out.push({ value: `${t.id}::${m}`, label: `${t.display_name} · ${m}` });
      }
    }
    return out;
  }, [templatesProviders, hasKey]);

  const modelLabel = current
    ? `${templatesProviders.find((t) => t.id === current.providerId)?.display_name ?? current.providerId} · ${current.model}`
    : '未选择';

  const parseModel = (): { providerId: string; model: string } | null => {
    if (!modelSel) return null;
    const [providerId, model] = modelSel.split('::');
    return providerId && model ? { providerId, model } : null;
  };

  const needClarify = !!intent && intent.questions.length > 0;

  const begin = async () => {
    if (busy) return;
    if (!topic.trim()) {
      setError('请先填写学习主题');
      return;
    }
    setError(null);
    setBusy(true);
    try {
      if (!intent) {
        // 第一次点击:preflight 抽取意图(带上面板已填字段,不再追问),缺字段则内联追问
        await preflight(topic.trim(), lang, {
          level: level || null,
          dailyMinutes: dailyMinutes || null,
          deadline: deadline || null,
        });
      }
      const it = useStudyStore.getState().intent;
      if (!it) return;
      if (it.questions.length > 0) {
        // 需要澄清:面板展示追问控件,等用户补充后再次点击
        return;
      }
      // 生效值:追问答案 > 面板字段 > 意图原值
      const effLevel = answers.level || level || it.level;
      const effMinutes = Number(answers.daily_minutes) || dailyMinutes || it.daily_minutes;
      const effDeadline = answers.deadline || deadline || it.deadline;
      const merged: StudyIntent = {
        ...it,
        topic: topic.trim() || it.topic,
        level: effLevel,
        daily_minutes: effMinutes,
        deadline: effDeadline,
        missing: [],
        questions: [],
      };
      await start({
        topic: merged.topic,
        intentJson: JSON.stringify(merged),
        dailyMinutes: effMinutes || null,
        deadline: effDeadline || null,
        lang,
        modelOverride: parseModel(),
      });
      setAnswers({});
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AsideDrawer>
      <aside className="pg-aside rp-config drawer">
        <div className="pg-aside-head">
          <h2>
            <IconGraduation size={18} />
            学习规划
          </h2>
          <p>点标题栏「面板」可开合本栏;描述你想学什么,AI 生成阶段化学习计划。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">学习主题</div>
          <textarea
            className="pg-field"
            style={{ width: '100%', minHeight: 64, resize: 'vertical' }}
            placeholder="例:我想在 3 个月内入门 Rust,能读懂开源项目代码"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
          />
        </div>

        <div className="pg-sec">
          <div className="pg-lab">基础信息</div>
          <div className="pg-row">
            <span>目标水平</span>
            <select className="pg-field" value={level} onChange={(e) => setLevel(e.target.value)}>
              <option value="">自动判断</option>
              <option value="zero">零基础</option>
              <option value="beginner">初学者</option>
              <option value="intermediate">中级</option>
              <option value="advanced">高级</option>
            </select>
          </div>
          <div className="pg-row">
            <span>每天投入(分钟)</span>
            <input
              className="pg-field num"
              type="number"
              min={5}
              max={600}
              value={dailyMinutes}
              onChange={(e) => setDailyMinutes(Math.max(0, Number(e.target.value) || 0))}
            />
          </div>
          <div className="pg-row">
            <span>目标完成日</span>
            <input
              className="pg-field"
              type="date"
              value={deadline}
              onChange={(e) => setDeadline(e.target.value)}
            />
          </div>
          <div className="pg-row">
            <span>输出语言</span>
            <select className="pg-field" value={lang} onChange={(e) => setLang(e.target.value)}>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>

        {needClarify && intent && (
          <div className="pg-warn">
            <div style={{ marginBottom: 6 }}>还有几个信息想确认:</div>
            {intent.questions.map((q, i) => {
              const f = intent.missing[i] ?? intent.missing[0] ?? '';
              return (
                <div key={i} style={{ marginBottom: 8 }}>
                  <div style={{ fontSize: 'var(--font-sub)', marginBottom: 4 }}>{q}</div>
                  {f === 'level' ? (
                    <select
                      className="pg-field"
                      style={{ width: '100%' }}
                      value={answers.level ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, level: e.target.value }))}
                    >
                      <option value="">请选择…</option>
                      <option value="zero">零基础</option>
                      <option value="beginner">初学者</option>
                      <option value="intermediate">中级</option>
                      <option value="advanced">高级</option>
                    </select>
                  ) : f === 'daily_minutes' ? (
                    <input
                      className="pg-field num"
                      type="number"
                      min={5}
                      max={600}
                      style={{ width: '100%' }}
                      placeholder="如 30"
                      value={answers.daily_minutes ?? ''}
                      onChange={(e) =>
                        setAnswers((a) => ({ ...a, daily_minutes: e.target.value }))
                      }
                    />
                  ) : (
                    <input
                      className="pg-field"
                      type="date"
                      style={{ width: '100%' }}
                      value={answers.deadline ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, deadline: e.target.value }))}
                    />
                  )}
                </div>
              );
            })}
            <div style={{ fontSize: 'var(--font-sub)', marginTop: 4 }}>
              补充后点击「开始生成」;留空将按默认值处理。
            </div>
          </div>
        )}

        <div className="pg-sec">
          <div className="pg-lab">执行模型</div>
          <select
            className="pg-field"
            style={{ width: '100%' }}
            value={modelSel}
            onChange={(e) => setModelSel(e.target.value)}
            title={modelSel ? undefined : modelLabel}
          >
            <option value="">跟随当前 · {modelLabel}</option>
            {modelOptions.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>

        <div className="pg-warn">
          规划流程要连续调用模型(意图澄清 → 知识地图 → 阶段设计 → 资料推荐),比较消耗
          token,请注意预算。
        </div>

        <div className="pg-foot">
          <button className="pg-run" disabled={busy || starting} onClick={() => void begin()}>
            <IconPlay size={14} />
            {busy ? '分析中…' : needClarify ? '带答案开始生成' : intent ? '开始生成' : '生成学习计划'}
          </button>
          {needClarify && (
            <button
              className="btn-ghost"
              onClick={() => {
                clearIntent();
                setAnswers({});
              }}
            >
              重新填写
            </button>
          )}
        </div>
        {error && <div className="pg-warn">{error}</div>}
      </aside>
    </AsideDrawer>
  );
}

/* ---------------- 列表视图 ---------------- */

function PlanList() {
  const { list, listLoaded, refreshList, openPlan, remove } = useStudyStore();
  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">学习规划</span>
        <span className="pg-hint">在左侧面板描述主题,生成阶段化学习计划</span>
      </div>
      <div className="rp-hist-list">
        {listLoaded && list.length === 0 && (
          <div className="rp-empty">还没有学习计划</div>
        )}
        {list.map((p) => (
          <PlanItem key={p.id} plan={p} onOpen={() => void openPlan(p.id)} onRemove={() => void remove(p.id)} />
        ))}
      </div>
    </div>
  );
}

function PlanItem({ plan, onOpen, onRemove }: { plan: PlanRow; onOpen: () => void; onRemove: () => void }) {
  return (
    <div className="rp-hist-item" onClick={onOpen}>
      <span
        className={`stat-dot ${
          plan.status === 'active' || plan.status === 'done'
            ? 'ok'
            : plan.status === 'error'
              ? 'err'
              : plan.status === 'cancelled'
                ? 'warn'
                : 'info'
        }`}
      />
      <div className="rp-hist-main">
        <div className="rp-hist-topic ellipsis">{plan.topic}</div>
        <div className="rp-hist-meta">
          {plan.status === 'generating' && <span className="mode-tag staged">生成中</span>}
          {plan.repaired && <span className="mode-tag concise">降级</span>}
          <span>{levelText(plan.level)}</span>
          <span>· 每天 {plan.daily_minutes} 分钟</span>
          {plan.deadline && <span>· 截止 {plan.deadline}</span>}
          <span>· {fmtDate(plan.created)}</span>
          <span>· {STATUS_LABEL[plan.status] ?? plan.status}</span>
        </div>
      </div>
      <button
        className="icon-btn"
        title="删除"
        onClick={(e) => {
          e.stopPropagation();
          if (confirm(`删除学习计划「${plan.topic}」?`)) onRemove();
        }}
      >
        <IconTrash size={14} />
      </button>
    </div>
  );
}

/* ---------------- 计划详情 / 运行视图 ---------------- */

function PlanView() {
  const nav = useNavigate();
  const { active, pause, resume, cancelRun, closePlan } = useStudyStore();
  const [nodeStates, setNodeStates] = useState<NodeState[]>([]);
  const [today, setToday] = useState<TodayPlan | null>(null);
  const [stats, setStats] = useState<StudyStats | null>(null);
  const [learn, setLearn] = useState<{ nodeId: number; name: string } | null>(null);
  const [quiz, setQuiz] = useState<{ nodeId: number; name: string } | null>(null);

  const id = active?.id ?? '';
  const running = active?.status === 'generating';
  const hasPayload = !!active?.payload;

  const loadStates = useCallback(async () => {
    if (!id || !hasPayload) return;
    try {
      const [ns, td, st] = await Promise.all([
        studyIpc.studyNodes(id),
        studyIpc.studyToday(id),
        studyIpc.studyStats(id),
      ]);
      setNodeStates(ns);
      setToday(td);
      setStats(st);
    } catch {
      /* 状态读取失败静默,主视图仍可用 */
    }
  }, [id, hasPayload]);

  useEffect(() => {
    if (!running) void loadStates();
  }, [running, loadStates]);

  if (!active) return null;
  const paused = active.trail.some((e) => e.type === 'paused');
  const resumedAfterPause =
    paused && active.trail.slice(active.trail.map((e) => e.type).lastIndexOf('paused') + 1).some((e) => e.type === 'resumed');
  const isPaused = paused && !resumedAfterPause;
  const nodes = buildStudyTimeline(active.trail, isPaused);
  const payload = active.payload;

  const warn = (e: unknown) => alert(e instanceof Error ? e.message : String(e));

  const deepDig = async (nodeName: string) => {
    try {
      const prefill = await studyIpc.studyDeepDig(active.id, nodeName);
      sessionStorage.setItem(RESEARCH_DRAFT_KEY, JSON.stringify({ topic: prefill }));
      nav('/research');
    } catch (e) {
      warn(e);
    }
  };

  const reviewDone = async (nodeId: number, correct: boolean) => {
    try {
      await studyIpc.studyReviewDone(active.id, nodeId, correct);
      void loadStates();
    } catch (e) {
      warn(e);
    }
  };

  return (
    <div className="rp-run">
      <div className="rp-head">
        <button className="btn-ghost" onClick={closePlan}>
          <IconArrowLeft size={14} />
          返回
        </button>
        <span className="topic" title={active.topic}>
          {active.topic}
        </span>
        {/* 载荷就绪(done)不代表学习完成——真实状态以列表徽标为准,此处仅在 运行/异常/终止 时显示 */}
        {!running && active.status === 'done' ? null : (
          <span className={`mode-tag ${running ? 'staged' : active.status === 'error' ? 'concise' : 'legacy'}`}>
            {STATUS_LABEL[running ? 'generating' : active.status] ?? active.status}
          </span>
        )}
        {payload?.repaired && <span className="mode-tag concise">降级结构</span>}
        <span className="sp" style={{ flex: 1 }} />
        {payload && <ExportBtn id={active.id} topic={payload.topic} />}
        {running && (
          <>
            {isPaused ? (
              <button className="btn-ghost" onClick={() => void resume()}>
                <IconPlay size={14} />
                继续
              </button>
            ) : (
              <button className="btn-ghost" onClick={() => void pause()}>
                <IconPause size={14} />
                暂停
              </button>
            )}
            <button className="btn-ghost stop" onClick={() => void cancelRun()}>
              <IconStop size={14} />
              终止
            </button>
          </>
        )}
      </div>

      {active.error && <div className="pg-warn">{active.error}</div>}

      <div className="rp-body">
        <div className="rp-main">
          {learn && payload ? (
            <LearnView
              planId={active.id}
              nodeId={learn.nodeId}
              nodeName={learn.name}
              onClose={() => setLearn(null)}
              onChanged={() => void loadStates()}
              onQuiz={() => {
                // 保留学习视图作为「上一级」:关闭测验弹窗后回到当前学习材料,而非计划详情
                if (learn) setQuiz({ nodeId: learn.nodeId, name: learn.name });
              }}
            />
          ) : payload ? (
            <PlanDetail
              payload={payload}
              states={nodeStates}
              today={today}
              stats={stats}
              onLearn={(nodeId, name) => setLearn({ nodeId, name })}
              onQuiz={(nodeId, name) => setQuiz({ nodeId, name })}
              onDeepDig={(n) => void deepDig(n)}
              onReview={(nodeId, ok) => void reviewDone(nodeId, ok)}
              onChanged={() => void loadStates()}
            />
          ) : (
            <div className="rp-timeline">
              {nodes.length === 0 && <div className="rp-empty">正在启动规划…</div>}
              {nodes.map((n) => (
                <div key={n.key} className={`rp-tl ${n.state}`}>
                  <div className="tt">
                    {n.title}
                    <span className={`pill${n.state === 'doing' ? ' warn' : ''}`}>
                      {n.state === 'doing' ? (isPaused ? '已暂停' : '运行中') : '完成'}
                    </span>
                  </div>
                  {n.lines.map((l, i) => (
                    <div key={i} className="rp-queries">
                      <span>{l}</span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {quiz && (
        <QuizDialog
          planId={active.id}
          nodeId={quiz.nodeId}
          nodeName={quiz.name}
          onClose={() => setQuiz(null)}
          onSubmitted={() => void loadStates()}
        />
      )}
    </div>
  );
}

/* ---------------- 导出按钮 ---------------- */

function ExportBtn({ id, topic }: { id: string; topic: string }) {
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  return (
    <span style={{ display: 'inline-flex', gap: 8, alignItems: 'center' }}>
      {hint && <span className="pg-hint ellipsis">{hint}</span>}
      <button
        className="btn-ghost"
        disabled={busy}
        onClick={async () => {
          if (busy) return;
          setBusy(true);
          setHint(null);
          try {
            const pick = await pickSavePath('导出学习计划(Markdown)', `${safeExportName(topic)}.md`, 'Markdown 文档', 'md');
            if (!pick) return;
            const doExport = (overwrite: boolean) => studyIpc.studyExportMarkdown(id, pick.token, overwrite);
            let saved: string;
            try {
              saved = await doExport(pick.existed);
            } catch (e) {
              const msg = e instanceof Error ? e.message : String(e);
              if (msg === EXPORT_TARGET_EXISTS) {
                if (!window.confirm(`目标文件已存在:\n${pick.path}\n\n确认覆盖?`)) return;
                saved = await doExport(true);
              } else {
                throw e;
              }
            }
            setHint(saved ? `已导出:${saved}` : null);
          } catch (e) {
            setHint(`导出失败:${e instanceof Error ? e.message : String(e)}`);
          } finally {
            setBusy(false);
          }
        }}
      >
        <IconDownload size={14} />
        {busy ? '导出中…' : '导出'}
      </button>
    </span>
  );
}

/* ---------------- 计划总览(今日待办 + 阶段树 + 资料 + 排期 + 统计) ---------------- */

function PlanDetail({
  payload,
  states,
  today,
  stats,
  onLearn,
  onQuiz,
  onDeepDig,
  onReview,
  onChanged,
}: {
  payload: PlanPayload;
  states: NodeState[];
  today: TodayPlan | null;
  stats: StudyStats | null;
  onLearn: (nodeId: number, name: string) => void;
  onQuiz: (nodeId: number, name: string) => void;
  onDeepDig: (node: string) => void;
  onReview: (nodeId: number, correct: boolean) => void;
  onChanged: () => void;
}) {
  const stateMap = useMemo(() => new Map(states.map((s) => [s.name, s])), [states]);
  return (
    <div className="rp-timeline">
      <div className="pg-sec">
        <div className="pg-lab">计划总览</div>
        <div className="pg-chips">
          <span className="pg-chip on">{levelText(payload.level)}</span>
          <span className="pg-chip">每天 {payload.daily_minutes} 分钟</span>
          {payload.deadline && <span className="pg-chip">截止 {payload.deadline}</span>}
          <span className="pg-chip">共 {payload.schedule.length} 周</span>
          <span className="pg-chip">{payload.nodes.length} 个知识点</span>
        </div>
        {payload.goal && <div className="pg-hint">学习目标:{payload.goal}</div>}
        {payload.repaired && (
          <div className="pg-warn">本计划为校验未通过后的降级结构,建议人工复核阶段划分。</div>
        )}
      </div>

      {/* 今日待办(P1):当前排期周应学知识点 + 到期复习 */}
      <div className="pg-sec">
        <div className="pg-lab">今日待办</div>
        {today && today.week > 0 ? (
          <>
            <div className="pg-chips">
              <span className="pg-chip on">
                第 {today.week} / {today.total_weeks} 周
              </span>
              {today.stage_names.map((n) => (
                <span key={n} className="pg-chip">
                  {n}
                </span>
              ))}
            </div>
            {today.reviews.length > 0 && (
              <div className="rp-queries" style={{ marginTop: 8 }}>
                {today.reviews.map((r) => (
                  <span key={r.id} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b style={{ fontWeight: 500 }}>复习:{r.node_name}</b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>
                      {r.overdue ? '已过期' : '今日到期'} · 隔 {r.interval_days} 天
                    </i>
                    <button className="pg-chip" title="还记得:间隔晋级" onClick={() => onReview(r.node_id, true)}>
                      <IconCheck size={12} />
                      记得
                    </button>
                    <button className="pg-chip" title="忘了:回到 1 天后再复习" onClick={() => onReview(r.node_id, false)}>
                      忘了
                    </button>
                  </span>
                ))}
              </div>
            )}
            {today.nodes.length > 0 ? (
              <div className="rp-queries" style={{ marginTop: 8 }}>
                {today.nodes.map((n) => (
                  <span key={n.node_id} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b style={{ fontWeight: 500 }}>{n.node_name}</b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>D{n.difficulty}</i>
                    <button className="pg-chip" onClick={() => onLearn(n.node_id, n.node_name)}>
                      学习
                    </button>
                  </span>
                ))}
              </div>
            ) : (
              <div className="pg-hint">本周排期知识点都已完成;可提前学习后面阶段,或等复习到期。</div>
            )}
          </>
        ) : (
          <div className="pg-hint">暂无排期:计划生成后按周展开,此处显示当日应学与到期复习。</div>
        )}
      </div>

      {payload.stages.map((s) => {
        const res = payload.resources.find((r) => r.stage === s.index);
        const weeks = payload.schedule.filter((w) => w.stage === s.index);
        const isCurrent = !!today && today.stage_names.includes(s.name);
        return (
          <div key={s.index} className="rp-tl ok">
            <div className="tt">
              阶段 {s.index + 1}:{s.name}
              {isCurrent && <span className="pill">本周</span>}
              <span className="pill">难度 {s.difficulty}</span>
              <span className="pill">约 {s.estimated_weeks} 周</span>
            </div>
            {s.goal && <div className="dd">{s.goal}</div>}
            <div className="rp-queries">
              {s.topics.map((t) => {
                const node = payload.nodes.find((n) => n.name === t);
                const st = stateMap.get(t);
                return (
                  <span key={t} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b
                      style={{
                        fontWeight: node?.must_learn ? 600 : 400,
                        color: node?.must_learn ? 'var(--text-1)' : 'var(--text-3)',
                      }}
                    >
                      {t}
                    </b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>D{node?.difficulty ?? '-'}</i>
                    {st?.done ? (
                      <span className="pill">已完成</span>
                    ) : st && st.mastery > 0 ? (
                      <span className="pill">掌握 {Math.round(st.mastery * 100)}%</span>
                    ) : null}
                    <button
                      className="pg-chip"
                      title="阅读该知识点的自学材料"
                      onClick={() => st && onLearn(st.node_id, t)}
                      disabled={!st}
                    >
                      学习
                    </button>
                    <button
                      className="pg-chip"
                      title="针对该知识点出 5 道测验题"
                      onClick={() => st && onQuiz(st.node_id, t)}
                      disabled={!st}
                    >
                      测验
                    </button>
                    <button
                      className="pg-chip"
                      title="针对该知识点发起一次深挖研究"
                      onClick={() => onDeepDig(t)}
                    >
                      <IconResearch size={13} />
                      深挖
                    </button>
                  </span>
                );
              })}
            </div>
            {s.milestone && (
              <div className="dd">
                <b>里程碑</b>:{s.milestone}
              </div>
            )}
            {weeks.length > 0 && (
              <div className="pg-hint">
                排期:第 {weeks[0].week}-{weeks[weeks.length - 1].week} 周 · 每天 {payload.daily_minutes} 分钟
              </div>
            )}
            {res && res.items.length > 0 && (
              <div className="rp-queries">
                {res.items.map((it, i) => (
                  <span key={i}>{it}</span>
                ))}
              </div>
            )}
          </div>
        );
      })}

      {/* 学习统计(P2):打卡 / 时长 / 完成度 + 专注计时 */}
      <StatsSec stats={stats} onChanged={onChanged} />
    </div>
  );
}

/* ---------------- 学习统计 + 专注计时(P2) ---------------- */

function StatsSec({ stats, onChanged }: { stats: StudyStats | null; onChanged: () => void }) {
  const { active } = useStudyStore();
  const [focusKey, setFocusKey] = useState(0);
  if (!stats || !active) return null;
  return (
    <div className="pg-sec">
      <div className="pg-lab">学习统计</div>
      <div className="pg-chips">
        <span className="pg-chip on">连续打卡 {stats.streak_days} 天</span>
        <span className="pg-chip">累计 {stats.total_days} 天</span>
        <span className="pg-chip">专注 {stats.total_minutes} 分钟</span>
        <span className="pg-chip">
          完成 {stats.done_nodes}/{stats.total_nodes}
        </span>
        <span className="pg-chip">平均掌握 {Math.round(stats.mastery_avg * 100)}%</span>
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(35, 1fr)',
          gap: 3,
          marginTop: 10,
          maxWidth: 560,
        }}
      >
        {stats.days.map((d) => {
          const on = d.activity > 0;
          return (
            <span
              key={d.date}
              title={`${d.date}${on ? ` · ${d.activity} 次学习行为` : ' · 未打卡'}`}
              style={{
                height: 10,
                borderRadius: 3,
                background: on ? 'var(--ok)' : 'var(--border-strong)',
                opacity: on ? Math.min(1, 0.55 + d.activity * 0.12) : 0.6,
              }}
            />
          );
        })}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
        <FocusTimer
          key={focusKey}
          planId={active.id}
          onLogged={() => {
            setFocusKey((k) => k + 1);
            onChanged();
          }}
        />
        <span className="pg-hint">
          打卡 = 当日有任意学习行为(学习 / 测验 / 复习 / 专注 / 深挖);专注计时随开随停,结束自动记录时长。
        </span>
      </div>
    </div>
  );
}

function FocusTimer({ planId, onLogged }: { planId: string; onLogged: () => void }) {
  const [running, setRunning] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => window.clearInterval(t);
  }, [running]);

  const mmss = `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;

  const stop = async (log: boolean) => {
    setRunning(false);
    const minutes = Math.floor(seconds / 60);
    setSeconds(0);
    if (!log || minutes < 1) return;
    setBusy(true);
    try {
      await studyIpc.studyFocusLog(planId, null, minutes);
      onLogged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <span style={{ display: 'inline-flex', gap: 8, alignItems: 'center' }}>
      {running && <span className="pg-chip on">专注中 {mmss}</span>}
      {!running ? (
        <button className="pg-chip" onClick={() => setRunning(true)}>
          <IconPlay size={12} />
          开始专注
        </button>
      ) : (
        <>
          <button className="pg-chip" disabled={busy} title="满 1 分钟才计入时长" onClick={() => void stop(true)}>
            结束并记录
          </button>
          <button className="pg-chip" disabled={busy} onClick={() => void stop(false)}>
            放弃
          </button>
        </>
      )}
    </span>
  );
}

/* ---------------- 学习视图(自学材料) ---------------- */

function LearnView({
  planId,
  nodeId,
  nodeName,
  onClose,
  onChanged,
  onQuiz,
}: {
  planId: string;
  nodeId: number;
  nodeName: string;
  onClose: () => void;
  onChanged: () => void;
  onQuiz: () => void;
}) {
  const [unit, setUnit] = useState<StudyUnit | null>(null);
  const [busy, setBusy] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [freeForm, setFreeForm] = useState(false);
  const [rated, setRated] = useState(false);

  const load = useCallback(
    async (force: boolean) => {
      setBusy(true);
      setErr(null);
      try {
        const prov = resolveProvider();
        const u = await studyIpc.studyUnitGenerate({
          id: planId,
          node_id: nodeId,
          free_form: freeForm,
          force,
          ...prov,
        });
        setUnit(u);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [planId, nodeId, freeForm],
  );

  useEffect(() => {
    void load(false);
  }, [load]);

  const rate = async (rating: number) => {
    try {
      await studyIpc.studyMasterySet(planId, nodeId, rating);
      setRated(true);
      onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  };

  const markDone = async () => {
    try {
      await studyIpc.studyNodeDone(planId, nodeId, true);
      onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="rp-timeline">
      <div className="pg-sec" style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <button className="btn-ghost" onClick={onClose}>
          <IconArrowLeft size={14} />
          返回
        </button>
        <b>{nodeName}</b>
        <span className="pg-chip">自学材料{unit ? ` · 约 ${unit.est_minutes} 分钟` : ''}</span>
        <span className="sp" style={{ flex: 1 }} />
        <button
          className={`pg-chip${freeForm ? ' on' : ''}`}
          title="重新生成时不套固定小节结构,按内容自然组织"
          onClick={() => setFreeForm((v) => !v)}
        >
          自由结构
        </button>
        <button className="pg-chip" disabled={busy} onClick={() => void load(true)}>
          重新生成
        </button>
        <button className="pg-chip" onClick={onQuiz}>
          测验
        </button>
      </div>
      {err && <div className="pg-warn">{err}</div>}
      {busy && <div className="pg-hint">正在生成自学材料,约需十几秒…</div>}
      {!busy && unit && (
        <div className="md report-md" style={{ marginTop: 8 }}>
          <MemoMarkdown content={unit.content_md} />
        </div>
      )}
      {unit && (
        <div className="pg-sec" style={{ marginTop: 12 }}>
          <div className="pg-lab">读完自评掌握</div>
          <div className="pg-chips">
            <button className="pg-chip" title="还没理解,稍后再来" onClick={() => void rate(0)}>
              没懂
            </button>
            <button className="pg-chip" title="大致理解了" onClick={() => void rate(0.5)}>
              大概懂
            </button>
            <button className="pg-chip" title="已掌握,标记知识点完成" onClick={() => void rate(1)}>
              掌握
            </button>
            <button className="pg-chip" onClick={() => void markDone()}>
              标记完成
            </button>
          </div>
          {rated && (
            <div className="pg-hint">
              已记录。掌握度 = 自评 ×0.4 + 测验 ×0.6,做一次测验更能校准;「掌握」同时把知识点标记为完成。
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ---------------- 测验弹窗(P1) ---------------- */

function QuizDialog({
  planId,
  nodeId,
  nodeName,
  onClose,
  onSubmitted,
}: {
  planId: string;
  nodeId: number;
  nodeName: string;
  onClose: () => void;
  onSubmitted: () => void;
}) {
  const [quiz, setQuiz] = useState<StudyQuiz | null>(null);
  const [busy, setBusy] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [picked, setPicked] = useState<Record<number, string>>({});
  const [shortAns, setShortAns] = useState<Record<number, string>>({});
  const [selfGrade, setSelfGrade] = useState<Record<number, boolean>>({});
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const [result, setResult] = useState<QuizResult | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    let alive = true;
    void (async () => {
      setBusy(true);
      setErr(null);
      try {
        const prov = resolveProvider();
        const q = await studyIpc.studyQuizNew({ id: planId, node_id: nodeId, ...prov });
        if (alive) {
          setQuiz(q);
          setBusy(false);
        }
      } catch (e) {
        if (alive) {
          setErr(e instanceof Error ? e.message : String(e));
          setBusy(false);
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [planId, nodeId]);

  const all = quiz?.questions ?? [];
  const answered = (q: (typeof all)[number]) =>
    q.type === 'short' ? selfGrade[q.idx] !== undefined : !!picked[q.idx];

  const submit = async () => {
    if (!quiz || submitting) return;
    setSubmitting(true);
    setErr(null);
    try {
      const answers = all.map((q) => ({
        question_idx: q.idx,
        user_answer: q.type === 'short' ? (shortAns[q.idx] ?? '') : (picked[q.idx] ?? ''),
        self_correct: q.type === 'short' ? (selfGrade[q.idx] ?? null) : null,
      }));
      const r = await studyIpc.studyQuizSubmit(planId, quiz.id, answers);
      setResult(r);
      onSubmitted();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const answerText = (idx: number) => {
    const q = all.find((x) => x.idx === idx);
    if (!q) return '';
    if (q.type === 'short') {
      const g = selfGrade[idx];
      const a = (shortAns[idx] ?? '').trim() || '(未作答)';
      return `${a} · 自评:${g === true ? '记得' : g === false ? '没记住' : '未评'}`;
    }
    return picked[idx] ?? '(未作答)';
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(720px, 92vw)', maxHeight: '84vh', display: 'flex', flexDirection: 'column' }}
      >
        <div className="dialog-head">
          <h2>
            知识测验 · {nodeName}
          </h2>
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="dialog-body" style={{ overflowY: 'auto', padding: '12px 16px' }}>
          {busy && <div className="pg-hint">正在出题…</div>}
          {err && <div className="pg-warn">{err}</div>}
          {result ? (
            <>
              <div className="pg-chips" style={{ marginBottom: 8 }}>
                <span className="pg-chip on">得分 {Math.round(result.score * 100)}%</span>
                <span className="pg-chip">掌握度 {Math.round(result.mastery * 100)}%</span>
                {result.score >= 0.8 && <span className="pg-chip">复习已排到下一间隔</span>}
                {result.score < 0.8 && <span className="pg-chip">明天安排复习</span>}
              </div>
              {all.map((q) => {
                const g = result.results.find((x) => x.question_idx === q.idx);
                return (
                  <div key={q.idx} className="pg-sec">
                    <div style={{ display: 'flex', gap: 6, alignItems: 'baseline' }}>
                      <span
                        className="pill"
                        style={g?.correct ? undefined : { color: 'var(--warn)', borderColor: '#EBD9B8', background: '#FCF6EA' }}
                      >
                        {g?.correct ? '对' : '错'}
                      </span>
                      <b>
                        {q.idx + 1}. {q.stem}
                      </b>
                    </div>
                    <div className="pg-hint">
                      参考答案:{g?.expected}
                      {g?.explanation ? ` · ${g.explanation}` : ''}
                    </div>
                    <div className="pg-hint">你的作答:{answerText(q.idx)}</div>
                  </div>
                );
              })}
            </>
          ) : (
            quiz && (
              <>
                {all.map((q) => (
                  <div key={q.idx} className="pg-sec">
                    <div>
                      <b>
                        {q.idx + 1}. {q.stem}
                      </b>{' '}
                      <span className="pg-hint">
                        {q.type === 'choice' ? '单选' : q.type === 'judge' ? '判断' : '简答'}
                      </span>
                    </div>
                    {q.type === 'choice' && (
                      <div className="pg-chips" style={{ marginTop: 6 }}>
                        {q.options.map((op) => {
                          const letter = op.trim().charAt(0).toUpperCase();
                          return (
                            <button
                              key={op}
                              className={`pg-chip${picked[q.idx] === letter ? ' on' : ''}`}
                              onClick={() => setPicked((p) => ({ ...p, [q.idx]: letter }))}
                            >
                              {op}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {q.type === 'judge' && (
                      <div className="pg-chips" style={{ marginTop: 6 }}>
                        <button
                          className={`pg-chip${picked[q.idx] === '对' ? ' on' : ''}`}
                          onClick={() => setPicked((p) => ({ ...p, [q.idx]: '对' }))}
                        >
                          对
                        </button>
                        <button
                          className={`pg-chip${picked[q.idx] === '错' ? ' on' : ''}`}
                          onClick={() => setPicked((p) => ({ ...p, [q.idx]: '错' }))}
                        >
                          错
                        </button>
                      </div>
                    )}
                    {q.type === 'short' && (
                      <div style={{ marginTop: 6 }}>
                        <textarea
                          className="pg-field"
                          style={{ width: '100%', minHeight: 56, resize: 'vertical' }}
                          placeholder="写下你的答案…"
                          value={shortAns[q.idx] ?? ''}
                          onChange={(e) => setShortAns((m) => ({ ...m, [q.idx]: e.target.value }))}
                        />
                        {!revealed[q.idx] ? (
                          <button
                            className="pg-chip"
                            style={{ marginTop: 6 }}
                            onClick={() => setRevealed((m) => ({ ...m, [q.idx]: true }))}
                          >
                            显示参考答案并自评
                          </button>
                        ) : (
                          <div style={{ marginTop: 6 }}>
                            <div className="pg-hint">参考:{q.answer}</div>
                            <div className="pg-chips">
                              <button
                                className={`pg-chip${selfGrade[q.idx] === true ? ' on' : ''}`}
                                onClick={() => setSelfGrade((m) => ({ ...m, [q.idx]: true }))}
                              >
                                记得
                              </button>
                              <button
                                className={`pg-chip${selfGrade[q.idx] === false ? ' on' : ''}`}
                                onClick={() => setSelfGrade((m) => ({ ...m, [q.idx]: false }))}
                              >
                                没记住
                              </button>
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                ))}
                <div className="pg-hint">
                  选择/判断自动判分;简答题对照参考答案自评。交卷后按「自评 ×0.4 + 测验 ×0.6」更新掌握度,并自动安排复习。
                </div>
              </>
            )
          )}
        </div>
        <div className="pg-foot" style={{ padding: '10px 16px' }}>
          <span className="pg-hint">{quiz && !result ? `已答 ${all.filter(answered).length}/${all.length}` : ''}</span>
          <span className="sp" style={{ flex: 1 }} />
          <button className="btn-ghost" onClick={onClose}>
            {result ? '关闭' : '取消'}
          </button>
          {!result && (
            <button className="btn-primary" disabled={busy || submitting || !quiz} onClick={() => void submit()}>
              {submitting ? '判分中…' : '交卷'}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/* ---------------- 页面 ---------------- */

export default function StudyPage() {
  const active = useStudyStore((s) => s.active);
  const refreshList = useStudyStore((s) => s.refreshList);
  const listTimer = useRef<number | undefined>(undefined);

  // 列表有计划在生成中时轮询刷新(打开详情走 openPlan,这里只刷列表行)
  useEffect(() => {
    void refreshList();
    if (active?.status === 'generating') {
      listTimer.current = window.setInterval(() => void refreshList(), 3000);
      return () => window.clearInterval(listTimer.current);
    }
  }, [refreshList, active?.status]);

  return (
    <div className="pg">
      <PlanPanel />
      <main className="pg-main">{active ? <PlanView key={active.id || 'draft'} /> : <PlanList />}</main>
    </div>
  );
}
