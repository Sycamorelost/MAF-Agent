/**
 * 计划详情 / 运行视图:生成时间线、计划总览、学习视图与测验弹窗的编排。
 */
import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { IconArrowLeft, IconPause, IconPlay, IconStop } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import { useSessionsStore } from '../../stores/sessions';
import { useProvidersStore } from '../../stores/providers';
import * as ipc from '../../lib/ipc';
import type { NodeState, StudyStats, StudyTask, TodayPlan } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';
import { STATUS_LABEL } from './PlanList';
import { ExportBtn, PlanDetail } from './PlanDetail';
import { LearnView } from './LearnView';
import { QuizDialog } from './QuizDialog';
import { buildStudyTimeline } from './timeline';

export function PlanView() {
  const nav = useNavigate();
  const { active, pause, resume, cancelRun, closePlan } = useStudyStore();
  const [nodeStates, setNodeStates] = useState<NodeState[]>([]);
  const [today, setToday] = useState<TodayPlan | null>(null);
  const [stats, setStats] = useState<StudyStats | null>(null);
  const [tasks, setTasks] = useState<StudyTask[]>([]);
  const [learn, setLearn] = useState<{ nodeId: number; name: string } | null>(null);
  const [quiz, setQuiz] = useState<{ nodeId: number; name: string } | null>(null);

  const id = active?.id ?? '';
  const running = active?.status === 'generating';
  const hasPayload = !!active?.payload;

  const loadStates = useCallback(async () => {
    if (!id || !hasPayload) return;
    try {
      const [ns, td, st, tk] = await Promise.all([
        studyIpc.studyNodes(id),
        studyIpc.studyToday(id),
        studyIpc.studyStats(id),
        studyIpc.studyTasksToday(id),
      ]);
      setNodeStates(ns);
      setToday(td);
      setStats(st);
      setTasks(tk);
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

  // 「深入研究」:研究页已移除,改为新建普通会话并把主题预填进输入框(同划词提问路径)
  const deepDig = async (nodeName: string) => {
    try {
      const prefill = await studyIpc.studyDeepDig(active.id, nodeName);
      const current = useProvidersStore.getState().current;
      const { id } = await ipc.createSession(
        `深入学习 ${nodeName}`,
        current ? `${current.providerId}/${current.model}` : '',
        null,
      );
      await useSessionsStore.getState().refresh();
      useSessionsStore.getState().setPendingComposer({ sessionId: id, text: prefill });
      nav(`/chat/${id}`);
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

  // 勾选每日任务:乐观更新,失败回滚
  const toggleTask = async (taskId: number, done: boolean) => {
    setTasks((ts) => ts.map((t) => (t.id === taskId ? { ...t, status: done ? 'done' : 'pending' } : t)));
    try {
      await studyIpc.studyTaskDone(active.id, taskId, done);
      void loadStates();
    } catch (e) {
      setTasks((ts) => ts.map((t) => (t.id === taskId ? { ...t, status: done ? 'pending' : 'done' } : t)));
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
        <span className="sp" />
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
              tasks={tasks}
              onTaskToggle={(taskId, done) => void toggleTask(taskId, done)}
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
