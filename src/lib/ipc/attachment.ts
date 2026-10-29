import { invoke } from '@tauri-apps/api/core';

import type { AttachmentMeta } from './types';

// ============ attachment(迭代 D1)============

/** 单附件大小上限,与后端 save_attachment 的 20MB 护栏保持一致。 */
export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024;
/** 单条消息附件数量上限,防止 base64 IPC 与请求体无限放大(M-10)。 */
export const MAX_ATTACHMENTS = 10;

/**
 * 校验待加入的附件文件:空文件 / 20MB 上限 / 数量上限。
 * existing 为当前已携带的附件数;返回首个错误信息,null 表示通过。
 */
export function validateAttachmentFiles(
  files: Array<{ name: string; size: number }>,
  existing = 0,
): string | null {
  if (existing + files.length > MAX_ATTACHMENTS) {
    return `最多携带 ${MAX_ATTACHMENTS} 个附件(当前已有 ${existing} 个)`;
  }
  for (const f of files) {
    if (f.size <= 0) return `附件「${f.name}」为空文件`;
    if (f.size > MAX_ATTACHMENT_BYTES) return `附件「${f.name}」超过 20MB 上限`;
  }
  return null;
}

/** File → data URL(base64),供 saveAttachment 使用。 */
export function readFileAsDataURL(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as string);
    r.onerror = () => reject(r.error ?? new Error('读取文件失败'));
    r.readAsDataURL(f);
  });
}

/** 保存附件:base64 → 落盘 data/attachments/{sessionId}/ → 文本类立即提取文本。 */
export const saveAttachment = (sessionId: string, fileName: string, dataBase64: string) =>
  invoke<AttachmentMeta>('save_attachment', { sessionId, fileName, dataBase64 });

/** 只提取文本、不落盘:工作台拖入/选择文件后直接填入输入区。 */
export const extractTextPreview = (fileName: string, dataBase64: string) =>
  invoke<string>('extract_text_preview', { fileName, dataBase64 });

/** 读取附件字节返回 data URL(渲染历史图片附件用)。 */
export const readAttachmentData = (path: string) =>
  invoke<string>('read_attachment_data', { path });

// ============ 附件管理中心(迭代 F7)============

/** 附件中心条目:messages.attachments JSON 展开 + 联表会话标题。 */
export interface AttachmentEntry {
  message_id: number;
  session_id: string;
  session_title: string;
  /** 来源消息所在分支(定位前需切换) */
  branch_id: string;
  /** 来源消息创建时间 */
  created: string;
  meta: AttachmentMeta;
}

/** 列出全部历史附件(按消息 id 倒序)。 */
export const listAllAttachments = () => invoke<AttachmentEntry[]>('list_all_attachments');

/** 把附件中心已有文件再次引入某会话(复用磁盘文件,重新生成元数据)。 */
export const insertAttachmentToSession = (sessionId: string, path: string) =>
  invoke<AttachmentMeta>('insert_attachment_to_session', { sessionId, path });

/** 删除附件:从来源消息 JSON 移除条目;全库无引用时清理磁盘文件。 */
export const deleteAttachment = (messageId: number, path: string) =>
  invoke<void>('delete_attachment', { messageId, path });



