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

/** 提取文本超过该字符数且已分块时,改注入「文档卡」(R3 C1:长文档不再全量灌入)。 */
export const DOC_CARD_THRESHOLD = 12000;

/** 文档注入策略:resident = 智能体 doc_pipeline=resident,所有已分块文档一律走文档卡。 */
export interface DocPipelineOpts {
  resident?: boolean;
}

/** 单个附件的注入文本:图片占位 / 文档卡 / 全文 / 失败提示(F7)。
 *  seen 用于「同一文档在本次请求中只注入一次」——历史里重复出现的附件改注入占位,
 *  避免 24 条滑窗把同一份文档重复灌 24 遍(R4 token 优化)。 */
function renderAttachment(
  m: AttachmentMeta,
  opts: DocPipelineOpts,
  seen?: Set<string>,
): string {
  if (m.kind === 'image') return `[图片附件:${m.name}]`;
  const key = m.doc_id ?? m.path;
  if (seen?.has(key)) {
    return `[附件:${m.name}(前文已注入;需要细节用 doc_search / doc_read 回看)]`;
  }
  seen?.add(key);
  const text = m.extracted_text ?? '';
  const chunked = (m.chunk_count ?? 0) > 0;
  if (chunked && (opts.resident || text.length > DOC_CARD_THRESHOLD)) {
    const head = `【文档:${m.name}(已驻留 ${m.chunk_count} 块${
      m.doc_id ? ` · doc_id ${m.doc_id}` : ''
    })】`;
    const sum = m.summary ? `\n摘要:${m.summary}` : '';
    return `${head}${sum}\n全文已分块入库,细节请用 doc_search(query=…) 检索、doc_read(doc=…, start=…) 回看任意章节;未检索到的内容不要臆测。`;
  }
  if (text) return `【附件:${m.name}】\n${text}`;
  if (m.extract_note) return `[附件:${m.name} 警告:${m.extract_note}]`;
  return `[附件:${m.name}]`;
}

/** 拼接附件注入文本(历史消息)。同一文档在本次请求内只注入一次,
 *  其余出现位置降级为占位,避免重复消耗 token。 */
export function injectAttachments(
  msg: MessageRow,
  opts: DocPipelineOpts = {},
  seen?: Set<string>,
): string {
  const metas = parseAttachments(msg.attachments);
  if (metas.length === 0) return msg.content;
  const parts = msg.content ? [msg.content] : [];
  for (const m of metas) parts.push(renderAttachment(m, opts, seen));
  return parts.join('\n\n');
}

/** 构建带滑窗与附件注入的历史(系统提示算 1 条;历史消息条数 = maxMsg,0=全部)。 */
export function buildSendHistory(
  messages: MessageRow[],
  maxMsg: number,
  system: string,
  extraUser: { content: string; images?: string[] } | null,
  docOpts: DocPipelineOpts = {},
): { role: string; content: string; images?: string[] }[] {
  let base = messages.slice();
  if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
  // 附件注入:窗口内所有带附件消息均注入,但同一文档只注入一次
  const seen = new Set<string>();
  const history = base.map((m) => ({
    role: m.role,
    content: injectAttachments(m, docOpts, seen),
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
  latest: MessageRow[], maxMsg: number, system: string, docOpts: DocPipelineOpts = {},
): { role: string; content: string }[] {
  let base = latest.slice();
  if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
  const seen = new Set<string>();
  const history = base.map((m) => ({
    role: m.role,
    content: injectAttachments(m, docOpts, seen),
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
  docOpts: DocPipelineOpts = {},
): { llmContentBase: string; images: string[] } {
  // F9:LLM 注入侧把 [@note:..]/[@memory:..] 占位替换为检索片段正文;展示侧保留原始输入
  const userParts = text ? [resolveAtRefs(text)] : [];
  const images: string[] = [];
  for (const a of atts) {
    if (a.kind === 'image') {
      const du = imgs[a.path];
      if (du) images.push(du);
      userParts.push(`[图片附件:${a.name}]`);
    } else {
      userParts.push(renderAttachment(a, docOpts));
    }
  }
  return { llmContentBase: userParts.join('\n\n'), images };
}
