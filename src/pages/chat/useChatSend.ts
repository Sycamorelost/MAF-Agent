import { useCallback, useEffect, useRef, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import type { BubbleMessage } from '../../components/MessageBubble';
import { useMessagesStore } from '../../stores/messages';
import { useMemoryStore } from '../../stores/memory';
import { useSessionsStore } from '../../stores/sessions';
import { useThinkingStore } from '../../stores/thinking';
import type { AttachmentMeta, AgentTemplate, MessageRow, MemoryStrategy, SessionRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { composeSystemPrompt, compressMemoryIfOverBudget } from './memoryContext';
import { CHAT_CANCEL_EVENT } from '../../lib/chatCancel';
import { DEFAULT_SYSTEM_PROMPT, withBase } from '../../lib/systemPrompt';
import { buildUserContent, buildSendHistory, rebuildSendHistory } from './attachments';
import { useChatSettings } from './useChatSettings';
import { useChatStream } from './useChatStream';
import { useMessageActions } from './useMessageActions';
import { describeSendError, toErrorMessage } from './streamErrors';
export interface ChatSendParams {
  sessionId: string | undefined;
  session: SessionRow | undefined;
  current: { providerId: string; model: string } | null;
  agent: AgentTemplate | null;
  messages: MessageRow[];
  /** 会话记忆正文(构建 system 用) */
  memoryBody: string;
  composerText: string;
  setComposerText: Dispatch<SetStateAction<string>>;
  pendingAttachments: AttachmentMeta[];
  pendingImages: Record<string, string>;
  setPendingAttachments: Dispatch<SetStateAction<AttachmentMeta[]>>;
  setPendingImages: Dispatch<SetStateAction<Record<string, string>>>;
  /** F4:从指定消息分叉并重载消息/分支 */
  forkAndReload: (msgId: number) => Promise<void>;
}
export function useChatSend({
  sessionId, session, current, agent, messages, memoryBody,
  composerText, setComposerText, pendingAttachments, pendingImages,
  setPendingAttachments, setPendingImages, forkAndReload,
}: ChatSendParams) {
  const { appendUser, startStreaming, endStreaming } = useMessagesStore();
  const loadMemory = useMemoryStore((s) => s.load);
  const setWebSearch = useSessionsStore((s) => s.setWebSearch);
  const thinkingSel = useThinkingStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const [sending, setSending] = useState(false);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 记忆自动压缩提示(D3.5) */
  const [compressNotice, setCompressNotice] = useState<string | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  // 工作台运行状态条/底部状态栏的「停止」按钮通过窗口事件请求取消(与 Composer 停止同一入口)
  useEffect(() => {
    const onCancelReq = (e: Event) => {
      if ((e as CustomEvent<string>).detail === sessionId) cancelRef.current?.();
    };
    window.addEventListener(CHAT_CANCEL_EVENT, onCancelReq);
    return () => window.removeEventListener(CHAT_CANCEL_EVENT, onCancelReq);
  }, [sessionId]);
  /** F5:换模型重试的目标消息 */
  const [retryTarget, setRetryTarget] = useState<BubbleMessage | null>(null);
  /** F8:钉入记忆后的轻提示 */
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2200);
  }, []);
  // 模型能力 / 学习进度 / 上下文窗口 / 全裸模式 / 物理契约等设置
  const { ctxWindow, bareMode, customContract, thinkCap, studyDigest } = useChatSettings(current);
  /** P5 / D19:会话生效预设的上下文条数上限(会话未单独设置时优先于全局滑窗;0 = 未解析/解析失败回落)。 */
  const [presetCtxMax, setPresetCtxMax] = useState(0);
  useEffect(() => {
    if (!sessionId) {
      setPresetCtxMax(0);
      return;
    }
    let alive = true;
    ipc
      .resolveSessionPreset(sessionId)
      .then((info) => {
        if (alive) setPresetCtxMax(info.preset.context.max_messages);
      })
      .catch(() => {
        if (alive) setPresetCtxMax(0);
      });
    return () => {
      alive = false;
    };
  }, [sessionId, session?.preset_id, session?.preset_version, agent?.agent_id]);
  /** 上次做过记忆预算检查的正文快照:内容没变就不重复检查(避免每轮同步等待提炼调用) */
  const lastBudgetMemRef = useRef<string | null>(null);
  /** 用指定记忆正文 + 策略组 system(交接确认时用最新值,不等 state 生效)。
   *  B4:sessions.system_prompt 是唯一基座事实源(绑定 Agent / 应用为提示词时
   *  后端已把渲染变量的 prompt 写入);agent 模板原文仅在旧数据(绑定但会话 prompt 为空)时兜底。 */
  const buildSystemWith = useCallback(
    (memBody: string, strategy: MemoryStrategy): string => {
      const core =
        session?.system_prompt?.trim() ||
        agent?.system_prompt ||
        DEFAULT_SYSTEM_PROMPT;
      // Base 分层(R3 A1):人格来自会话/智能体提示,系统层只追加 [身份锚(全裸模式可关)] + 物理契约
      return composeSystemPrompt(withBase(core, { identity: !bareMode, customContract }), memBody, strategy, studyDigest);
    },
    [session?.system_prompt, agent, studyDigest, bareMode, customContract],
  );
  const buildSystem = useCallback(
    (): string => buildSystemWith(memoryBody, session?.memory_injection_strategy ?? 'hybrid'),
    [memoryBody, session?.memory_injection_strategy, buildSystemWith],
  );
  // 流式发送子流程(组装请求 / 写流式缓冲 / 落库助手消息 / 自动提炼)
  const runStream = useChatStream({
    sessionId, current, agent, thinkCap, thinkingSel, setSending, setError, cancelRef,
  });
  /** 用新分支的最新历史组装 system + 滑窗 + 附件注入(F4 重新生成 / F5 换模型重试共用)。 */
  const rebuildHistory = (): { role: string; content: string }[] => {
    if (!sessionId) return [];
    const latest = useMessagesStore.getState().bySession[sessionId] ?? [];
    return rebuildSendHistory(
      latest,
      session?.max_context_messages || presetCtxMax || ctxWindow,
      buildSystem(),
      // C2:doc_pipeline=resident 的智能体一律注入文档卡,细节交给 doc_search/doc_read
      { resident: agent?.doc_pipeline === 'resident' },
    );
  };
  // 消息操作与来源/钉入记忆
  const {
    onTranslate, onQuote, onEditResend, onDelete, onRegenerate, onPickRetryModel,
    onSaveSources, onPin,
  } = useMessageActions({
    sessionId, setComposerText, setPendingAttachments, setPendingImages,
    setError, showToast, forkAndReload, runStream, rebuildHistory, retryTarget, setRetryTarget,
  });

  // ---------- D3.5:记忆预算护栏(发送前检测,超限自动压缩) ----------
  const enforceMemoryBudget = async () => {
    if (!sessionId || !current) return;
    // 记忆正文未变则跳过:避免每轮都排队一次「提炼」LLM 调用,徒增首字延迟
    if (lastBudgetMemRef.current === memoryBody) return;
    lastBudgetMemRef.current = memoryBody;
    const notice = await compressMemoryIfOverBudget(sessionId, current, loadMemory);
    if (notice) setCompressNotice(notice);
  };
  /** 构建带滑窗与附件注入的历史(系统提示算 1 条;历史消息条数 = maxMsg,0=全部)。
   *  条数优先级(D19):会话单独设置 > 会话生效预设 > 全局设置(默认 24 条)。 */
  const buildHistory = (extraUser: { content: string; images?: string[] } | null) =>
    buildSendHistory(
      messages,
      session?.max_context_messages || presetCtxMax || ctxWindow,
      buildSystem(),
      extraUser,
      { resident: agent?.doc_pipeline === 'resident' },
    );
  /** overrideText:首页草稿直发用(绕过输入框状态);overrideAtt 同理携带首页已落盘的附件。
   *  发送失败时回填输入框。 */
  const onSend = async (
    overrideText?: string,
    overrideAtt?: { metas: AttachmentMeta[]; images: Record<string, string> },
  ) => {
    if (!sessionId) return;
    const atts = overrideAtt?.metas ?? pendingAttachments;
    const imgs = overrideAtt?.images ?? pendingImages;
    const text = (overrideText ?? composerText).trim();
    if ((!text && atts.length === 0) || sending) return;
    // 会话可能已被空对话清理逻辑删除(如离开后返回),发送前校验存在性
    if (!session) {
      setError('会话不存在或已被删除，请返回首页重新创建对话');
      if (overrideText !== undefined) setComposerText(text);
      return;
    }
    if (!current) {
      setError('请先在右上角配置 API Key 并选择模型');
      return;
    }
    // 即时响应:先点亮「正在思考」占位,再做联网搜索/落库/记忆预算等耗时准备;
    // 正文/思考内容等流式 chunk 到达后才填充显示(runStream 内的 startStreaming 为幂等复位)
    startStreaming(sessionId);
    setSending(true);
    try {
      // 1. 组装用户消息原始内容(用于 db 展示)与注入内容(用于 LLM)
      // F9:LLM 注入侧把 [@note:..]/[@memory:..] 占位替换为检索片段正文;展示侧保留原始输入
      const { llmContentBase: base, images } = buildUserContent(text, atts, imgs, {
        resident: agent?.doc_pipeline === 'resident',
      });
      // R4:长文档本地预检索 —— 把与本次提问最相关的若干块直接注入(块数随会话预设 top_k),
      // 不依赖「工具总开关」与「模型是否主动调 doc_search」。失败不影响发送。
      let llmContentBase = base;
      if (text) {
        try {
          const hit = await ipc.docPrefetch(sessionId, text);
          if (hit) llmContentBase = `${base}\n\n${hit}`;
        } catch {
          // 预检索失败时退回纯文档卡/全文注入
        }
      }
      const displayContent = text; // 前端展示只存原始输入,附件在元数据卡片展示
      const attachmentsJson = atts.length > 0 ? JSON.stringify(atts) : null;
      // 联网搜索(开关存会话;失败时降级为普通对话并提示)
      let sources: ipc.SourceItem[] | null = null;
      let llmContent = llmContentBase;
      if (session?.web_search_enabled && text) {
        setSearching(true);
        try {
          const res = await ipc.searchTest(text);
          if (res.length > 0) {
            sources = res;
            const list = res.map((s) => `[${s.id}] ${s.title}\n${s.url}\n${s.snippet}`).join('\n\n');
            llmContent = `${llmContentBase}\n\n【联网搜索结果】\n${list}\n\n请在回答中引用上述来源时用 [n] 标注(如 [1][2]),回答末尾无需列出来源。`;
          }
        } catch (e) {
          setError(`联网搜索失败:${toErrorMessage(e)}(本次按无联网继续)`);
        } finally {
          setSearching(false);
        }
      }
      // 2. 持久化用户消息
      await appendUser(sessionId, displayContent, current?.model ?? null, attachmentsJson);
      setComposerText('');
      setPendingAttachments([]);
      setPendingImages({});
      // 2.5 D3.5 记忆预算护栏:超限先压缩再发送
      await enforceMemoryBudget();
      // 3. 构建历史(system + 滑窗 + 附件注入)并流式
      await runStream(buildHistory({ content: llmContent, images }), sources);
    } catch (e) {
      // 会话在发送过程中被删除(如空会话清理竞态),提示用户重建
      setError(describeSendError(toErrorMessage(e)));
      // 首页草稿直发失败:回填输入框,内容不丢失
      if (overrideText !== undefined) setComposerText(text);
      // 即时占位清理:此时错误必来自 runStream 之前的准备步骤(runStream 自吞错误),
      // 若流式占位仍在则收掉,避免留下永久光标
      endStreaming(sessionId);
      setSending(false);
    }
  };
  const onCancel = () => {
    cancelRef.current?.();
    setSending(false);
  };
  const onRecall = () => {
    const lastUser = [...messages].reverse().find((m) => m.role === 'user');
    if (lastUser) setComposerText(lastUser.content);
  };
  /** 联网开关(会话级持久化) */
  const onToggleWebSearch = async () => {
    if (!session) return;
    await setWebSearch(session.id, !session.web_search_enabled);
  };
  // 附件增删与草稿交接见 useDraftHandoff
  return {
    sending, searching, error, setError, compressNotice, setCompressNotice, toast,
    retryTarget, setRetryTarget, thinkCap, buildSystemWith, onSend, onCancel, onTranslate,
    onQuote, onEditResend, onDelete, onRegenerate, onPickRetryModel, onSaveSources, onPin,
    onRecall, onToggleWebSearch,
  };
}
