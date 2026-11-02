import { useEffect, useRef } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { useAttachmentInsertStore } from '../../stores/attachmentInsert';
import { useSessionsStore } from '../../stores/sessions';
import type { AttachmentMeta } from '../../lib/ipc';
import type { SkippedFile } from '../../lib/attachmentDrop';
import * as ipc from '../../lib/ipc';
type DraftAttachments = { metas: AttachmentMeta[]; images: Record<string, string> };
interface DraftHandoffParams {
  sessionId: string | undefined;
  setComposerText: Dispatch<SetStateAction<string>>;
  pendingAttachments: AttachmentMeta[];
  pendingImages: Record<string, string>;
  setPendingAttachments: Dispatch<SetStateAction<AttachmentMeta[]>>;
  setPendingImages: Dispatch<SetStateAction<Record<string, string>>>;
  onSend: (text: string, att?: DraftAttachments) => void;
  onError: (msg: string) => void;
  /** 轻提示(拖入文件夹时的逐个文件拒绝原因),不占用错误栏 */
  onNotice: (msg: string) => void;
}
/** 输入框(草稿)交接与附件操作:附件中心插入队列 / 预填文本 / 首页草稿直发 / 附件增删。 */
export function useDraftHandoff({
  sessionId, setComposerText, pendingAttachments, pendingImages,
  setPendingAttachments, setPendingImages, onSend, onError, onNotice,
}: DraftHandoffParams) {
  // F7:消费附件中心「插入当前会话」队列,追加到输入框待发附件
  useEffect(() => {
    if (!sessionId) return;
    const items = useAttachmentInsertStore.getState().consume(sessionId);
    if (items?.length) setPendingAttachments((prev) => [...prev, ...items]);
  }, [sessionId]);
  // 预填文本:消费待填内容(如深入研究新建的会话跳转至此)
  useEffect(() => {
    if (!sessionId) return;
    const p = useSessionsStore.getState().pendingComposer;
    if (p && p.sessionId === sessionId) {
      setComposerText(p.text);
      useSessionsStore.getState().setPendingComposer(null);
    }
  }, [sessionId]);
  // 首页草稿接续:从首页创建会话后,一次性消费并**直接发送**(失败回填输入框,内容不丢失)
  // 附件随草稿一并交接(首页无法在会话创建前落盘)
  const draftSentRef = useRef(false);
  useEffect(() => {
    if (draftSentRef.current) return;
    const d = sessionStorage.getItem('metaagent.draft');
    const rawAtt = sessionStorage.getItem('metaagent.draft.att');
    let draftAtt: DraftAttachments | undefined;
    if (rawAtt) {
      sessionStorage.removeItem('metaagent.draft.att');
      try {
        draftAtt = JSON.parse(rawAtt) as DraftAttachments;
      } catch {
        draftAtt = undefined;
      }
    }
    if (draftAtt) {
      setPendingAttachments(draftAtt.metas);
      setPendingImages(draftAtt.images);
    }
    if (d) {
      sessionStorage.removeItem('metaagent.draft');
      draftSentRef.current = true;
      void onSend(d, draftAtt);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  /** 选中文件 → 落盘附件(图片同时保留完整 data URL 供 vision 注入)。
   *  notice 为拖入文件夹时的跳过/截断提示(黑名单、阈值、空文件);
   *  rejected 为逐文件拒绝明细,落盘留存供附件中心查看。 */
  const handlePickFiles = async (files: File[], notice?: string, rejected?: SkippedFile[]) => {
    if (!sessionId) return;
    if (rejected?.length) {
      void ipc
        .recordAttachmentRejects(
          sessionId,
          rejected.map((r) => ({ name: r.name, reason: r.reason, source: 'import' as const })),
        )
        .catch(() => {});
    }
    if (files.length === 0) {
      if (notice) onNotice(notice);
      return;
    }
    const picked = files;
    const sizeErr = ipc.validateAttachmentFiles(picked, pendingAttachments.length);
    if (sizeErr) {
      onError(sizeErr);
      return;
    }
    const newMetas: AttachmentMeta[] = [];
    const newImages: Record<string, string> = { ...pendingImages };
    for (const f of picked) {
      try {
        const b64 = await new Promise<string>((res, rej) => {
          const reader = new FileReader();
          reader.onload = () => {
            const r = reader.result as string;
            const i = r.indexOf(','); // 去掉 data:...;base64, 前缀
            res(i >= 0 ? r.slice(i + 1) : r);
          };
          reader.onerror = rej;
          reader.readAsDataURL(f);
        });
        const meta = await ipc.saveAttachment(sessionId, f.name, b64);
        newMetas.push(meta);
        if (meta.kind === 'image') {
          // mime 按扩展名推断
          const ext = f.name.split('.').pop()?.toLowerCase() ?? 'png';
          const mime = ext === 'jpg' ? 'image/jpeg' : `image/${ext}`;
          newImages[meta.path] = `data:${mime};base64,${b64}`;
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        void ipc
          .recordAttachmentRejects(sessionId, [
            { name: f.name, reason: msg, source: 'save' as const },
          ])
          .catch(() => {});
        onError(`附件 ${f.name} 上传失败: ${msg}`);
      }
    }
    if (newMetas.length) {
      setPendingAttachments((prev) => [...prev, ...newMetas]);
      setPendingImages(newImages);
    }
    if (notice) onNotice(notice);
  };
  const handleRemoveAttachment = (index: number) => {
    setPendingAttachments((prev) => {
      const next = [...prev];
      const removed = next.splice(index, 1)[0];
      if (removed?.kind === 'image') {
        setPendingImages((im) => {
          const ni = { ...im };
          delete ni[removed.path];
          return ni;
        });
      }
      return next;
    });
  };
  return { handlePickFiles, handleRemoveAttachment };
}
