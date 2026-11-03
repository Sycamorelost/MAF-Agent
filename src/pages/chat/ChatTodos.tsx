//! 会话任务清单条(harness todo 面板):实时展示模型分解的多步任务进度。
//! 数据源:`session_get_todos` 初始拉取 + `session-todos-{id}` 事件刷新;
//! 同时监听 `chat-notice-{id}` 把后端通知(如上下文自动压缩)转给对话页横幅。

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  chatNoticeEvent,
  sessionGetTodos,
  todosEvent,
  type TodoItem,
} from '../../lib/ipc/session';
import { IconChevronDown, IconChevronRight, IconRefresh } from '../../components/icons';

interface Props {
  sessionId: string;
  /** 把后端通知转给对话页已有横幅(compressNotice)展示。 */
  onNotice: (msg: string) => void;
  /** 隐藏清单本体(工作台模式由左栏任务计划展示),但仍保留事件订阅。 */
  hidden?: boolean;
}

export function ChatTodos({ sessionId, onNotice, hidden = false }: Props) {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  // 默认收起:与输入框同高的紧凑横条,点击展开看详细列表
  const [collapsed, setCollapsed] = useState(true);
  // 反向动画过渡态:点击收起后先播放 reverse,播完再真正卸载
  const [closing, setClosing] = useState(false);

  const DRAWER_ANIM_MS = 180;

  const openDrawer = () => {
    setClosing(false);
    setCollapsed(false);
  };
  const closeDrawer = () => {
    if (collapsed) return;
    setClosing(true);
    window.setTimeout(() => {
      setCollapsed(true);
      setClosing(false);
    }, DRAWER_ANIM_MS);
  };

  const refresh = () =>
    sessionGetTodos(sessionId)
      .then((v) => setTodos(v))
      .catch(() => {});

  // session 切换:清列表 + 重新拉取 + 订阅事件
  useEffect(() => {
    let alive = true;
    setTodos([]);
    const safeRefresh = () =>
      sessionGetTodos(sessionId)
        .then((v) => alive && setTodos(v))
        .catch(() => {});
    safeRefresh();
    const unTodo = listen(todosEvent(sessionId), safeRefresh);
    const unNotice = listen<string>(chatNoticeEvent(sessionId), (e) => onNotice(e.payload));
    return () => {
      alive = false;
      void unTodo.then((f) => f());
      void unNotice.then((f) => f());
    };
  }, [sessionId, onNotice]);

  // 抽屉展开时主动刷新一次(双保险,避免事件漏推/竞态)
  useEffect(() => {
    if (!collapsed) refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsed]);

  if (hidden || todos.length === 0) return null;
  const done = todos.filter((t) => t.status === 'completed').length;
  const total = todos.length;
  const inProgress = todos.find((t) => t.status === 'in_progress');
  const pendingCount = todos.filter((t) => t.status === 'pending').length;
  const cancelledCount = todos.filter((t) => t.status === 'cancelled').length;
  // 单行摘要:进行中任务 or 全部完成 or 下一个待办 or 第一条
  const summary = inProgress
    ? inProgress.content
    : done === total
      ? '全部任务已完成'
      : todos.find((t) => t.status === 'pending')?.content ?? todos[0]?.content ?? '';

  const statusLabel: Record<string, string> = {
    pending: '待执行',
    in_progress: '执行中',
    completed: '已完成',
    cancelled: '已取消',
  };
  const priorityLabel: Record<string, string> = { high: '高优先', medium: '中优先', low: '低优先' };

  return (
    <div className={`chat-todos${collapsed ? '' : ' is-open'}`}>
      {/* 紧凑触发头:点击展开/收起 */}
      <button
        type="button"
        className="chat-todos-head"
        onClick={() => (collapsed ? openDrawer() : closeDrawer())}
      >
        <span className="chat-todos-title">任务</span>
        <span className="chat-todos-count">
          {done}/{total}
        </span>
        <span className="chat-todos-summary" title={summary}>
          {done === total ? (
            <span className="done-text">✓ 全部已完成({done})</span>
          ) : inProgress ? (
            <>
              <span className="dot-sep">·</span>
              <span className="summary-label is-ongoing">进行中</span>
              {summary}
            </>
          ) : (
            <>
              <span className="dot-sep">·</span>
              {summary}
            </>
          )}
        </span>
        {collapsed ? <IconChevronRight size={14} /> : <IconChevronDown size={14} />}
      </button>
      {/* 抽屉面板:closing 态也要渲染以播放反向动画 */}
      {(!collapsed || closing) && (
        <div className={`chat-todos-drawer${closing ? ' is-closing' : ''}`} role="dialog">
          <div className="chat-todos-drawer-head">
            <div className="chat-todos-drawer-head-main">
              <span className="chat-todos-title">任务详情</span>
              <span className="chat-todos-count">
                {done}/{total}
              </span>
              {pendingCount > 0 && (
                <span className="todo-stat is-pending" title={`${pendingCount} 项待执行`}>
                  待 {pendingCount}
                </span>
              )}
              {cancelledCount > 0 && (
                <span className="todo-stat is-cancelled" title={`${cancelledCount} 项已取消`}>
                  取 {cancelledCount}
                </span>
              )}
            </div>
            <button
              type="button"
              className="chat-todos-refresh"
              title="刷新任务清单"
              onClick={(e) => {
                e.stopPropagation();
                refresh();
              }}
            >
              <IconRefresh size={14} />
            </button>
            <button
              type="button"
              className="chat-todos-close"
              title="收起"
              onClick={() => closeDrawer()}
            >
              <IconChevronDown size={14} />
            </button>
          </div>
          <ul className="chat-todos-list">
            {todos.map((t) => (
              <li
                key={t.id}
                className={`chat-todo is-${t.status}`}
                title={[
                  `状态:${statusLabel[t.status] ?? t.status}`,
                  `优先级:${priorityLabel[t.priority] ?? t.priority}`,
                  t.task_id ? `关联目标:${t.task_id}` : '',
                ]
                  .filter(Boolean)
                  .join(' | ')}
              >
                <i className={`todo-dot is-${t.status}`} />
                <span className="chat-todo-text">{t.content}</span>
                <span className="chat-todo-status">{statusLabel[t.status] ?? t.status}</span>
                {t.priority === 'high' && t.status !== 'completed' && (
                  <span className="chat-todo-prio">高</span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
