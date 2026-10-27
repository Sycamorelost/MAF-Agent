import { useCallback, useEffect, useRef, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { useNavigate } from 'react-router-dom';
import type { BubbleMessage } from '../../components/MessageBubble';
import { useMessagesStore } from '../../stores/messages';
import { useMemoryStore } from '../../stores/memory';
import { useSessionsStore } from '../../stores/sessions';
import { useThinkingStore, DEFAULT_THINKING_LEVEL } from '../../stores/thinking';
import type { AttachmentMeta, AgentTemplate, MessageRow, MemoryStrategy, SessionRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { studyMemoryDigest } from '../../lib/ipc/study';
import { mcpCollectTools } from '../../lib/ipc/mcp';
import { maybeAutoSummarize, composeSystemPrompt, compressMemoryIfOverBudget } from './memoryContext';
import { DEFAULT_SYSTEM_PROMPT, withProtocol } from '../../lib/systemPrompt';
import { useEditResend, buildUserContent, buildSendHistory, rebuildSendHistory } from './attachments';
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
  const nav = useNavigate();
  const { appendUser, appendAssistant, startStreaming, pushStreamDelta } = useMessagesStore();
  const { pushStreamReasoning, setStreamReasoningTokens, endStreaming, remove } = useMessagesStore();
  const loadMemory = useMemoryStore((s) => s.load);
  const setWebSearch = useSessionsStore((s) => s.setWebSearch);
  const thinkingSel = useThinkingStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const [sending, setSending] = useState(false);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 记忆自动压缩提示(D3.5) */
  const [compressNotice, setCompressNotice] = useState<string | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
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
  // 当前模型的思考能力(手动修正 > 内置规则表推断;查询失败按不支持处理,控件整体隐藏)
  const [thinkCap, setThinkCap] = useState<ipc.ReasoningCapability | null>(null);
  useEffect(() => {
    let alive = true;
    if (!current) {
      setThinkCap(null);
      return;
    }
    ipc.getReasoningCapability(current.providerId, current.model)
      .then((cap) => alive && setThinkCap(cap))
      .catch(() => alive && setThinkCap(null));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.providerId, current?.model]);
  // 学习进度摘要(P2 记忆注入):进页取一次,失败静默(学习板块为空时后端返回 null)
  const [studyDigest, setStudyDigest] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    studyMemoryDigest().then((d) => alive && setStudyDigest(d)).catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  /** 全局上下文窗口(条数):会话自身未设置(max_context_messages=0)时作为默认值。
   *  默认 24 条 ≈ 12 轮,避免长会话把全部历史原样重发导致 token 线性膨胀。 */
  const [ctxWindow, setCtxWindow] = useState(24);
  useEffect(() => {
    let alive = true;
    ipc
      .getSettings()
      .then((s) => alive && setCtxWindow(s.context_window_messages ?? 24))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
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
      // 所有会话统一追加能力协议(工具 / 记忆分工 / 诚实性 / 失败话术)
      return composeSystemPrompt(withProtocol(core), memBody, strategy, studyDigest);
    },
    [session?.system_prompt, agent, studyDigest],
  );
  const buildSystem = useCallback(
    (): string => buildSystemWith(memoryBody, session?.memory_injection_strategy ?? 'hybrid'),
    [memoryBody, session?.memory_injection_strategy, buildSystemWith],
  );
  /** F5:用指定模型(缺省当前模型)跑一次流式;modelOverride 只影响本次,不改全局/会话模型,
   *  新消息 model_id 记录为 override 模型。 */
  const runStream = useCallback(
    async (
      history: { role: string; content: string; images?: string[] }[],
      sources?: ipc.SourceItem[] | null,
      modelOverride?: { providerId: string; model: string },
    ) => {
      if (!sessionId) return;
      const eff = modelOverride ?? current;
      if (!eff) {
        setError('请先在右上角配置 API Key 并选择模型');
        return;
      }
      setSending(true);
      setError(null);
      try {
        startStreaming(sessionId);
        let tokens = 0;
        // 思考参数:模型不支持 → null(后端能力门控二次兜底);用户关闭 → 'off';否则档位
        const reasoningEffort = !thinkCap?.enabled
          ? null
          : thinkingSel && !thinkingSel.on
            ? 'off'
            : (thinkingSel?.level ?? DEFAULT_THINKING_LEVEL);
        // Phase 5e:收集已启用 MCP server 的工具定义;空则不启用工具调用(行为不变)
        let tools: ipc.ToolDef[] | undefined;
        try {
          const collected = await mcpCollectTools();
          if (collected.length > 0) tools = collected;
        } catch {
          /* MCP 未配置/未启动 —— 静默降级 */
        }
        const { done, cancel } = ipc.chatStream(
          {
            provider_id: eff.providerId, model: eff.model, messages: history,
            temperature: null, max_tokens: null, top_p: null,
            custom_base_url: null, custom_display_name: null,
            session_id: sessionId, reasoning_effort: reasoningEffort,
            tools,
          },
          (chunk) => {
            if (chunk.type === 'delta') pushStreamDelta(sessionId, chunk.content);
            // 思考流分流:独立缓冲去抖,折叠块展示,不进正文
            else if (chunk.type === 'reasoning') pushStreamReasoning(sessionId, chunk.content);
            // Phase 5e:工具执行结果 —— 以 maf-tool 围栏块并入正文(小字一行 + 折叠详情)
            // 性能:该正文会落库并随历史每轮重发,故单条结果截断到 1000 字符(原 4000)
            else if (chunk.type === 'tool_call_result') {
              const body = chunk.content.length > 1000
                ? `${chunk.content.slice(0, 1000)}\n…(已截断,完整结果见工具详情)`
                : chunk.content;
              const payload = JSON.stringify({
                s: chunk.server_name, t: chunk.tool_name, ok: chunk.ok, c: body,
              });
              pushStreamDelta(sessionId, `\n\n\`\`\`maf-tool\n${payload}\n\`\`\`\n\n`);
            }
            else if (chunk.type === 'done') {
              tokens = (chunk.prompt_tokens ?? 0) + (chunk.completion_tokens ?? 0);
              if (chunk.reasoning_tokens != null) {
                setStreamReasoningTokens(sessionId, chunk.reasoning_tokens);
              }
            }
          },
        );
        cancelRef.current = cancel;
        await done;
        const { content: fullText, reasoning } = endStreaming(sessionId);
        if (fullText) {
          await appendAssistant(
            sessionId, fullText, eff.model, tokens,
            sources && sources.length > 0 ? JSON.stringify(sources) : null, reasoning,
          );
        }
        maybeAutoSummarize(sessionId, eff.providerId, eff.model, agent?.memory_write ?? true);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        endStreaming(sessionId);
      } finally {
        setSending(false);
        cancelRef.current = null;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      sessionId, current, agent, thinkCap, thinkingSel, startStreaming, pushStreamDelta,
      pushStreamReasoning, setStreamReasoningTokens, endStreaming, appendAssistant,
    ],
  );
  /** 用新分支的最新历史组装 system + 滑窗 + 附件注入(F4 重新生成 / F5 换模型重试共用)。 */
  const rebuildHistory = (): { role: string; content: string }[] => {
    if (!sessionId) return [];
    const latest = useMessagesStore.getState().bySession[sessionId] ?? [];
    return rebuildSendHistory(latest, session?.max_context_messages || ctxWindow, buildSystem());
  };

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
   *  会话未单独设置(0)= 跟随全局设置(默认 24 条),避免长会话全量重发。 */
  const buildHistory = (extraUser: { content: string; images?: string[] } | null) =>
    buildSendHistory(
      messages,
      session?.max_context_messages || ctxWindow,
      buildSystem(),
      extraUser,
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
    try {
      // 1. 组装用户消息原始内容(用于 db 展示)与注入内容(用于 LLM)
      // F9:LLM 注入侧把 [@note:..]/[@memory:..] 占位替换为检索片段正文;展示侧保留原始输入
      const { llmContentBase, images } = buildUserContent(text, atts, imgs);
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
          setError(`联网搜索失败:${e instanceof Error ? e.message : String(e)}(本次按无联网继续)`);
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
      const msg = e instanceof Error ? e.message : String(e);
      // 会话在发送过程中被删除(如空会话清理竞态),提示用户重建
      if (msg.includes('FOREIGN KEY constraint failed')) {
        setError('会话已被清理，请返回首页重新创建对话');
      } else {
        setError(`发送失败: ${msg}`);
      }
      // 首页草稿直发失败:回填输入框,内容不丢失
      if (overrideText !== undefined) setComposerText(text);
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
