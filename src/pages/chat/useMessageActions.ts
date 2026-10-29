import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { useNavigate } from 'react-router-dom';
import type { BubbleMessage } from '../../components/MessageBubble';
import { useMessagesStore } from '../../stores/messages';
import { useMemoryStore } from '../../stores/memory';
import * as ipc from '../../lib/ipc';
import type { AttachmentMeta } from '../../lib/ipc';
import { useEditResend } from './attachments';
import type { RunChatStream } from './useChatStream';

interface MessageActionsParams {
  sessionId: string | undefined;
  setComposerText: Dispatch<SetStateAction<string>>;
  setPendingAttachments: Dispatch<SetStateAction<AttachmentMeta[]>>;
  setPendingImages: Dispatch<SetStateAction<Record<string, string>>>;
  setError: (msg: string | null) => void;
  showToast: (msg: string) => void;
  forkAndReload: (msgId: number) => Promise<void>;
  runStream: RunChatStream;
  rebuildHistory: () => { role: string; content: string }[];
  retryTarget: BubbleMessage | null;
  setRetryTarget: Dispatch<SetStateAction<BubbleMessage | null>>;
}

/** 消息操作(翻译/引用/编辑重发/删除/重新生成/换模型重试)与来源/钉入记忆的写入动作。 */
export function useMessageActions({
  sessionId, setComposerText, setPendingAttachments, setPendingImages,
  setError, showToast, forkAndReload, runStream, rebuildHistory, retryTarget, setRetryTarget,
}: MessageActionsParams) {
  const nav = useNavigate();
  const remove = useMessagesStore((s) => s.remove);
  const loadMemory = useMemoryStore((s) => s.load);

  // ---------- 消息操作(useCallback 稳定引用,配合 MessageList memo) ----------
  /** 「翻译」(D6.5):内容写入草稿并跳翻译页。 */
  const onTranslate = useCallback(
    (content: string) => {
      try {
        sessionStorage.setItem('translate_draft', JSON.stringify({ text: content }));
      } catch {
        // 存储失败不阻塞跳转
      }
      nav('/translate');
    },
    [nav],
  );
  const onQuote = useCallback((content: string) => {
    const quoted = content.split('\n').map((l) => `> ${l}`).join('\n');
    setComposerText((prev) => (prev ? `${quoted}\n\n${prev}` : `${quoted}\n\n`));
  }, []);
  const onEditResend = useEditResend({
    sessionId, setComposerText, setPendingAttachments, setPendingImages, forkAndReload,
  });
  const onDelete = useCallback(
    async (id: number) => {
      if (!sessionId) return;
      await remove(sessionId, id);
    },
    [sessionId, remove],
  );
  const onRegenerate = useCallback(
    async (msg: BubbleMessage) => {
      if (!sessionId) return;
      // F4:分叉替代物理删除;新分支可见历史 = 该 AI 消息之前的前缀
      await forkAndReload(msg.id);
      // 用新分支的最新历史重发(无新用户消息)
      await runStream(rebuildHistory());
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, forkAndReload, runStream],
  );
  /** 选定模型后:从该条 AI 消息处分叉并以新模型重发;取消弹窗 = 仅 setRetryTarget(null)。 */
  const onPickRetryModel = useCallback(
    async (providerId: string, model: string) => {
      const msg = retryTarget;
      setRetryTarget(null);
      if (!sessionId || !msg) return;
      // F4:分叉替代物理删除
      await forkAndReload(msg.id);
      await runStream(rebuildHistory(), null, { providerId, model });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, retryTarget, forkAndReload, runStream],
  );

  // ---------- 来源存入记忆(写入 memory.md「资料」节) ----------
  const onSaveSources = useCallback(
    async (sources: ipc.SourceItem[]) => {
      if (!sessionId) return;
      const lines = sources
        .map((s) => `- [${s.title || s.url}](${s.url})${s.snippet ? ` — ${s.snippet}` : ''}`)
        .join('\n');
      try {
        await ipc.appendMemory(sessionId, lines);
        await loadMemory(sessionId);
      } catch (e) {
        setError(`存入记忆失败:${e instanceof Error ? e.message : String(e)}`);
      }
    },
    [sessionId, loadMemory],
  );
  /** F8:一键钉入记忆(写入指定节,快照 note=pin_message) */
  const onPin = useCallback(
    async (content: string, section: string) => {
      if (!sessionId) return;
      try {
        await ipc.appendMemory(sessionId, content, section);
        await loadMemory(sessionId);
        showToast(`已钉入「${section}」`);
      } catch (e) {
        setError(`钉入记忆失败:${e instanceof Error ? e.message : String(e)}`);
      }
    },
    [sessionId, loadMemory, showToast],
  );
  return {
    onTranslate, onQuote, onEditResend, onDelete, onRegenerate, onPickRetryModel,
    onSaveSources, onPin,
  };
}
