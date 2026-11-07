import { invoke } from '@tauri-apps/api/core';

/** 会话产物(交付物)IPC。写入由后端 `present` 工具完成,前端只读。 */
export interface DeliverableDto {
  id: number;
  session_id: string;
  /** 绑定的助手消息 id;流式中可能为 null */
  message_id: number | null;
  /** image / doc / sheet / slides / pdf / archive / text / html / file */
  kind: string;
  /** 工作区相对路径或绝对路径 */
  path: string;
  name: string;
  mime: string | null;
  size: number | null;
  note: string | null;
  created: string;
}

export const listDeliverables = (sessionId: string) =>
  invoke<DeliverableDto[]>('list_deliverables', { sessionId });
