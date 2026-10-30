//! 运行状态条(P4):阶段 / 步骤进度 / 耗时 / 上下文占比 / 停止。不显示无意义 spinner,空闲时只留上下文占比。
//! 阶段口径(ui-design 约束):等待审批(审批窗口事件)> 执行(流式已出现工具块)
//! > 准备(已开流但尚无工具调用)> 完成(本轮刚结束)> 空闲。
//! 步数口径:进行中任务的计划步骤(完成数+1 / 总数),数据源为共享任务缓存(stores/tasks)。
//! 停止按钮经窗口事件 chat-cancel-request 送达 useChatSend(与 Composer 停止同一入口)。

import { useEffect, useRef, useState } from 'react';
import { IconStop } from '../../../components/icons';
import { requestChatCancel } from '../../../lib/chatCancel';
import { memoryBudgetStatus } from '../../../lib/ipc/memory';
import { useMessagesStore } from '../../../stores/messages';
import { runningTask, taskProgress, useTasksStore } from '../../../stores/tasks';

type Phase = 'idle' | 'preparing' | 'executing' | 'waiting' | 'done';

const PHASE_LABEL: Record<Phase, string> = {
  idle: '空闲',
  preparing: '准备中',
  executing: '执行中',
  waiting: '等待审批',
  done: '已完成',
};

/** 流式文本里已出现工具调用围栏,即进入执行阶段。 */
const TOOL_FENCE = /```maf-tool/;

function fmtElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m${String(s % 60).padStart(2, '0')}s`;
}

export function RunStatusBar({ sessionId }: { sessionId: string }) {
  const streamingText = useMessagesStore((s) => s.streaming[sessionId]);
  const streaming = streamingText !== undefined;
  const tasks = useTasksStore((s) => s.bySession[sessionId]);
  const loadTasks = useTasksStore((s) => s.load);
  const [waiting, setWaiting] = useState(false);
  const [finished, setFinished] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [budget, setBudget] = useState<{ tokens: number; limit: number } | null>(null);
  const startRef = useRef<number | null>(null);
  const prevStreaming = useRef(false);

  // 审批中状态由审批对话框的窗口事件驱动(无会话维度,streaming 结束时兜底清零)
  useEffect(() => {
    const onPending = (e: Event) => setWaiting((e as CustomEvent<boolean>).detail);
    window.addEventListener('mcp-approval-pending', onPending);
    return () => window.removeEventListener('mcp-approval-pending', onPending);
  }, []);

  // 计时:streaming 起止;结束后保留最后一次耗时,并短暂标记「完成」
  useEffect(() => {
    if (streaming) {
      prevStreaming.current = true;
      setFinished(false);
      startRef.current = Date.now();
      setElapsed(0);
      const t = setInterval(() => {
        if (startRef.current) setElapsed(Date.now() - startRef.current);
      }, 1000);
      return () => clearInterval(t);
    }
    setWaiting(false);
    if (prevStreaming.current) {
      prevStreaming.current = false;
      setFinished(true);
    }
    memoryBudgetStatus(sessionId)
      .then((b) => setBudget({ tokens: b.tokens, limit: b.limit }))
      .catch(() => {});
  }, [streaming, sessionId]);

  // 初始拉一次上下文占比与任务列表;流式结束时再拉一次任务(步骤可能已被勾选)
  useEffect(() => {
    memoryBudgetStatus(sessionId)
      .then((b) => setBudget({ tokens: b.tokens, limit: b.limit }))
      .catch(() => {});
  }, [sessionId]);

  useEffect(() => {
    if (!streaming) void loadTasks(sessionId);
  }, [sessionId, streaming, loadTasks]);

  // 切换会话时回到空闲
  useEffect(() => {
    setFinished(false);
    prevStreaming.current = false;
  }, [sessionId]);

  const phase: Phase = streaming
    ? waiting
      ? 'waiting'
      : TOOL_FENCE.test(streamingText ?? '')
        ? 'executing'
        : 'preparing'
    : finished
      ? 'done'
      : 'idle';
  const pct = budget && budget.limit > 0 ? Math.min(100, Math.round((budget.tokens / budget.limit) * 100)) : null;
  const running = runningTask(tasks);
  const prog = taskProgress(running);

  return (
    <div className={`runbar ph-${phase}`}>
      <span className="live">
        <i className="pulse" />
        {PHASE_LABEL[phase]}
        {phase !== 'idle' && running ? ` · ${running.title}` : ''}
      </span>
      {streaming && prog && (
        <span className="step">
          第 {Math.min(prog.done + 1, prog.total)} 步 / 共 {prog.total} 步
        </span>
      )}
      {streaming && elapsed >= 1000 && <span className="step">{fmtElapsed(elapsed)}</span>}
      <span className="sp" />
      {pct !== null && (
        <>
          <span className="step" title={`上下文 ${budget!.tokens}/${budget!.limit} tokens`}>
            上下文 {pct}%
          </span>
          <span className="ctxmini">
            <i style={{ width: `${pct}%` }} />
          </span>
        </>
      )}
      {streaming && (
        <button
          type="button"
          className="btn-ghost btn-xs"
          title="停止本轮生成"
          onClick={() => requestChatCancel(sessionId)}
        >
          <IconStop size={11} />
          停止
        </button>
      )}
    </div>
  );
}
