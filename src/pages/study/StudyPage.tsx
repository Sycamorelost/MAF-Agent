/**
 * 学习规划页面。
 *
 * 来源声明 / Attribution:
 *   Interaction flow derived from Deep-tutor — https://github.com/hershate/Deep-tutor
 *   Copyright (c) hershate and contributors
 *   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
 *   Re-implemented with MAF Agent's component system; no upstream CSS/assets copied.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { IconArrowLeft, IconPause, IconPlay, IconStop } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import type { NodeState, StudyEvent, StudyStats, TodayPlan } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';
import { PlanList, PlanPanel, STATUS_LABEL } from './PlanList';
import { ExportBtn, PlanDetail } from './PlanDetail';
import { LearnView } from './LearnView';
import { QuizDialog } from './QuizDialog';

/** 研究页草稿接手约定(见 ResearchPage 挂载逻辑) */
const RESEARCH_DRAFT_KEY = 'metaagent.research.draft';

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
