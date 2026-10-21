import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import * as ipc from '../../lib/ipc';
import type { AttachmentMeta, MessageRow } from '../../lib/ipc';
import type { BubbleMessage } from '../../components/MessageBubble';
import { resolveAtRefs } from '../../lib/atRefs';

/** 解析附件 JSON */
export function parseAttachments(raw: string | null): AttachmentMeta[] {
  if (!raw) return [];
  try {
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

/** 拼接附件注入文本(历史消息)。仅最近一条附件消息注入全文,其余仅标记。 */
export function injectAttachments(msg: MessageRow, isLastAttach: boolean): string {
  const metas = parseAttachments(msg.attachments);
  if (metas.length === 0) return msg.content;
  const parts = msg.content ? [msg.content] : [];
  for (const m of metas) {
    if (m.kind === 'image') parts.push(`[图片附件:${m.name}]`);
    else if (isLastAttach && m.extracted_text) parts.push(`【附件:${m.name}】\n${m.extracted_text}`);
    else parts.push(`[附件:${m.name}]`);
  }
  return parts.join('\n\n');
}

/** 找数组中最后一条带附件消息的下标(无则 -1)。 */
export function lastAttachIndex(list: MessageRow[]): number {
  for (let i = list.length - 1; i >= 0; i--) {
    if (parseAttachments(list[i].attachments).length > 0) return i;
  }
  return -1;
}

/** 构建带滑窗与附件注入的历史(系统提示算 1 条;历史消息条数 = maxMsg,0=全部)。 */
export function buildSendHistory(
  messages: MessageRow[],
  maxMsg: number,
  system: string,
  extraUser: { content: string; images?: string[] } | null,
): { role: string; content: string; images?: string[] }[] {
  let base = messages.slice();
  if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
  // 附件注入:仅范围内最后一条带附件的消息注入全文
  const lastIdx = lastAttachIndex(base);
  const history = base.map((m, idx) => ({
    role: m.role,
    content: injectAttachments(m, idx === lastIdx),
  }));
  const out: { role: string; content: string; images?: string[] }[] = [
    { role: 'system', content: system },
    ...history,
  ];
  if (extraUser) out.push({ role: 'user', content: extraUser.content, images: extraUser.images });
  return out;
}

/** 用新分支的最新历史组装 system + 滑窗 + 附件注入(F4 重新生成 / F5 换模型重试共用)。 */
export function rebuildSendHistory(
  latest: MessageRow[], maxMsg: number, system: string,
): { role: string; content: string }[] {
  let base = latest.slice();
  if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
  const lastIdx = lastAttachIndex(base);
  const history = base.map((m, idx) => ({
    role: m.role,
    content: injectAttachments(m, idx === lastIdx),
  }));
  return [{ role: 'system', content: system }, ...history];
}

/** 重读图片附件 data URL(失败忽略)。 */
export async function loadAttachmentImages(metas: AttachmentMeta[]): Promise<Record<string, string>> {
  const imgs: Record<string, string> = {};
  for (const m of metas) {
    if (m.kind === 'image') {
      try {
        imgs[m.path] = await ipc.readAttachmentData(m.path);
      } catch {
        // 忽略失败
      }
    }
  }
  return imgs;
}

/** 「编辑并重发」:回填草稿与附件(图片重读 data URL),再从该消息处分叉。 */
export function useEditResend({
  sessionId, setComposerText, setPendingAttachments, setPendingImages, forkAndReload,
}: {
  sessionId: string | undefined;
  setComposerText: Dispatch<SetStateAction<string>>;
  setPendingAttachments: Dispatch<SetStateAction<AttachmentMeta[]>>;
  setPendingImages: Dispatch<SetStateAction<Record<string, string>>>;
  forkAndReload: (msgId: number) => Promise<void>;
}) {
  return useCallback(
    async (msg: BubbleMessage) => {
      if (!sessionId) return;
      setComposerText(msg.content);
      const metas = parseAttachments(msg.attachments);
      setPendingAttachments(metas);
      // 图片附件重读 data URL 到 pendingImages
      setPendingImages(await loadAttachmentImages(metas));
      // F4:不再物理删除后续消息,从该消息处分叉(旧分支完整保留);
      // 用户随后发送的新消息自动落入新分支
      await forkAndReload(msg.id);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, forkAndReload],
  );
}

/** 组装用户消息注入内容(LLM 侧)。 */
export function buildUserContent(
  text: string,
  atts: AttachmentMeta[],
  imgs: Record<string, string>,
): { llmContentBase: string; images: string[] } {
  // F9:LLM 注入侧把 [@note:..]/[@memory:..] 占位替换为检索片段正文;展示侧保留原始输入
  const userParts = text ? [resolveAtRefs(text)] : [];
  const images: string[] = [];
  for (const a of atts) {
    if (a.kind === 'image') {
      const du = imgs[a.path];
      if (du) images.push(du);
      userParts.push(`[图片附件:${a.name}]`);
    } else if (a.extracted_text) userParts.push(`【附件:${a.name}】\n${a.extracted_text}`);
    else userParts.push(`[附件:${a.name}]`);
  }
  return { llmContentBase: userParts.join('\n\n'), images };
}
