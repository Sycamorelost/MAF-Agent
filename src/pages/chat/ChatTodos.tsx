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

interface Props {
  sessionId: string;
  /** 把后端通知转给对话页已有横幅(compressNotice)展示。 */
  onNotice: (msg: string) => void;
}

export function ChatTodos({ sessionId, onNotice }: Props) {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [collapsed, setCollapsed] = useState(false);

  useEffect(() => {
    let alive = true;
    setTodos([]);
    const refresh = () =>
      sessionGetTodos(sessionId)
        .then((v) => alive && setTodos(v))
        .catch(() => {});
    refresh();
    const unTodo = listen(todosEvent(sessionId), refresh);
    const unNotice = listen<string>(chatNoticeEvent(sessionId), (e) => onNotice(e.payload));
    return () => {
      alive = false;
      void unTodo.then((f) => f());
      void unNotice.then((f) => f());
    };
  }, [sessionId, onNotice]);

  if (todos.length === 0) return null;
  const done = todos.filter((t) => t.status === 'completed').length;

  return (
    <div className="chat-todos">
      <button type="button" className="chat-todos-head" onClick={() => setCollapsed(!collapsed)}>
        <span className="chat-todos-title">任务清单</span>
        <span className="chat-todos-count">
          {done}/{todos.length}
        </span>
        <span className="chat-todos-arrow">{collapsed ? '▸' : '▾'}</span>
      </button>
      {!collapsed && (
        <ul className="chat-todos-list">
          {todos.map((t) => (
            <li key={t.id} className={`chat-todo is-${t.status}`}>
              <i className={`todo-dot is-${t.status}`} />
              <span className="chat-todo-text">{t.content}</span>
              {t.priority === 'high' && t.status !== 'completed' && (
                <span className="chat-todo-prio">高</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
