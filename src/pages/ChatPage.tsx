import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import MessageBubble, { type BubbleMessage } from '../components/MessageBubble';
import StreamingBubble from '../components/StreamingBubble';
import Composer from '../components/Composer';
import { ResizeHandle } from '../components/ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
import MemPanel from '../components/MemPanel';
import MemoryHandoffDialog from '../components/MemoryHandoffDialog';
import ModelSwitchDialog from '../components/ModelSwitchDialog';
import AgentPickerDialog from '../components/AgentPickerDialog';
import { IconChat, IconCheck, IconChevronDown, IconEdit, IconPullRequest, IconTrash, IconX, IconBrain, agentIconComponent } from '../components/icons';
import { useMessagesStore } from '../stores/messages';
import { useSessionsStore } from '../stores/sessions';
import { useProvidersStore } from '../stores/providers';
import { useMemoryStore } from '../stores/memory';
import { useAgentsStore } from '../stores/agents';
import { useAttachmentInsertStore } from '../stores/attachmentInsert';
import { useThinkingStore, DEFAULT_THINKING_LEVEL } from '../stores/thinking';
import type { AttachmentMeta, MessageRow, MemoryStrategy } from '../lib/ipc';
import * as ipc from '../lib/ipc';
import { injectByStrategy, estimateTokens, enforceBudget, MEMORY_BUDGET_TOKENS } from '../lib/memoryInject';
import { resolveAtRefs } from '../lib/atRefs';
import { studyMemoryDigest } from '../lib/ipc/study';

/** 正在提炼的会话,避免并发重复提炼。 */
const summarizing = new Set<string>();

/** 需要时后台自动提炼记忆:needs_summarize → summarize_memory → write_memory → 刷新面板。
 *  D3.7:优先使用设置中的提炼专用小模型,缺省回退当前会话模型。
 *  B2:Agent 模板 memory_write=false 时跳过(如代码审查/翻译等不写记忆的智能体)。 */
async function maybeAutoSummarize(
  sessionId: string,
  providerId: string,
  model: string,
  memoryWrite: boolean,
) {
  if (!memoryWrite) return;
  if (summarizing.has(sessionId)) return;
  try {
    const need = await ipc.needsSummarize(sessionId);
    if (!need) return;
    summarizing.add(sessionId);
    // 提炼专用模型设置(缺省 = 当前会话模型);base_url 由后端按 provider_id 解析模板
    const settings = await ipc.getSettings().catch(() => null);
    const pid = settings?.summarize_provider_id || providerId;
    const m = settings?.summarize_model || model;
    const content = await ipc.summarizeMemory({
      session_id: sessionId,
      provider_id: pid,
      model: m,
      custom_base_url: null,
      custom_display_name: null,
    });
    await ipc.writeMemory(sessionId, content, 'auto_summarize');
    await useMemoryStore.getState().load(sessionId);
  } catch (e) {
    console.warn('[auto_summarize] failed:', e);
  } finally {
    summarizing.delete(sessionId);
  }
}

/** 解析附件 JSON */
function parseAttachments(raw: string | null): AttachmentMeta[] {
  if (!raw) return [];
  try {
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

/** 拼接附件注入文本(历史消息)。仅最近一条附件消息注入全文,其余仅标记。 */
function injectAttachments(msg: MessageRow, isLastAttach: boolean): string {
  const metas = parseAttachments(msg.attachments);
  if (metas.length === 0) return msg.content;
  const parts = msg.content ? [msg.content] : [];
  for (const m of metas) {
    if (m.kind === 'image') {
      parts.push(`[图片附件:${m.name}]`);
    } else if (isLastAttach && m.extracted_text) {
      parts.push(`【附件:${m.name}】\n${m.extracted_text}`);
    } else {
      parts.push(`[附件:${m.name}]`);
    }
  }
  return parts.join('\n\n');
}

/** 找数组中最后一条带附件消息的下标(无则 -1)。 */
function lastAttachIndex(list: MessageRow[]): number {
  for (let i = list.length - 1; i >= 0; i--) {
    if (parseAttachments(list[i].attachments).length > 0) return i;
  }
  return -1;
}

/** 稳定空数组:避免 `?? []` 每次渲染生成新引用导致 MessageList memo 失效。 */
const EMPTY_MESSAGES: MessageRow[] = [];

/** 分支显示名:用户自定义 > 默认(主分支 / 分支 xxxx)。 */
function branchLabel(b: ipc.BranchInfo | undefined, fallbackId: string): string {
  if (b?.name) return b.name;
  const id = b?.branch_id ?? fallbackId;
  return id === 'main' ? '主分支' : `分支 ${id.slice(0, 4)}`;
}

interface MessageListProps {
  messages: MessageRow[];
  lastAssistantId: number | undefined;
  onQuote: (content: string) => void;
  onTranslate: (content: string) => void;
  onEditResend: (msg: BubbleMessage) => void;
  onDelete: (id: number) => void;
  onRegenerate: (msg: BubbleMessage) => void;
  onRetryModel: (msg: BubbleMessage) => void;
  onSaveSources: (sources: ipc.SourceItem[]) => void;
  onPin: (content: string, section: string) => void;
}

/** 历史消息列表(memo):流式刷新时整棵历史子树跳过渲染(流式气泡由 StreamingBubble 独立订阅)。 */
const MessageList = memo(function MessageList({
  messages,
  lastAssistantId,
  onQuote,
  onTranslate,
  onEditResend,
  onDelete,
  onRegenerate,
  onRetryModel,
  onSaveSources,
  onPin,
}: MessageListProps) {
  return (
    <>
      {messages.map((m) => (
        <div key={m.id} id={`msg-${m.id}`}>
          <MessageBubble
            msg={m}
            isLastAssistant={m.role === 'assistant' && m.id === lastAssistantId}
            onQuote={onQuote}
            onTranslate={onTranslate}
            onEditResend={m.role === 'user' ? onEditResend : undefined}
            onDelete={onDelete}
            onRegenerate={
              m.role === 'assistant' && m.id === lastAssistantId ? () => onRegenerate(m) : undefined
            }
            onRetryModel={
              m.role === 'assistant' && m.id === lastAssistantId ? () => onRetryModel(m) : undefined
            }
            onSaveSources={m.role === 'assistant' ? onSaveSources : undefined}
            onPin={onPin}
          />
        </div>
      ))}
    </>
  );
});

export default function ChatPage() {
  const { id: sessionId } = useParams<{ id: string }>();
  const nav = useNavigate();
  const memPanel = usePanelWidth('mempanel', 320, 240, 520, 'right');
  const {
    bySession,
    load,
    appendUser,
    appendAssistant,
    startStreaming,
    pushStreamDelta,
    pushStreamReasoning,
    setStreamReasoningTokens,
    endStreaming,
    remove,
  } = useMessagesStore();
  // 流式状态用 boolean selector:只在开始/结束变化两次,ChatPage 不随 50ms 去抖重渲染
  const hasStreaming = useMessagesStore((s) =>
    sessionId ? s.streaming[sessionId] !== undefined : false,
  );
  const sessions = useSessionsStore((s) => s.sessions);
  const setWebSearch = useSessionsStore((s) => s.setWebSearch);
  const setSessionModel = useSessionsStore((s) => s.setModel);
  const setSessionAgent = useSessionsStore((s) => s.setAgent);
  const setActiveBranch = useSessionsStore((s) => s.setActiveBranch);
  const current = useProvidersStore((s) => s.current);
  const selectModel = useProvidersStore((s) => s.select);
  // 单模型配置(迭代 D8):切换目标模型的默认记忆注入策略
  const modelConfig = useProvidersStore((s) => s.modelConfig);
  const loadModelConfig = useProvidersStore((s) => s.loadModelConfig);
  const memoryDto = useMemoryStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const loadMemory = useMemoryStore((s) => s.load);
  const { templates: agentTemplates, load: loadAgents } = useAgentsStore();

  const [sending, setSending] = useState(false);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [composerText, setComposerText] = useState('');
  const [pendingAttachments, setPendingAttachments] = useState<AttachmentMeta[]>([]);
  const [pendingImages, setPendingImages] = useState<Record<string, string>>({});
  const [modelSwitchOpen, setModelSwitchOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<(() => void) | null>(null);

  const session = sessions.find((s) => s.id === sessionId);
  const setMemoryStrategy = useSessionsStore((s) => s.setMemoryStrategy);
  const messages = sessionId ? (bySession[sessionId] ?? EMPTY_MESSAGES) : EMPTY_MESSAGES;

  // F7:消费附件中心「插入当前会话」队列,追加到输入框待发附件
  useEffect(() => {
    if (!sessionId) return;
    const items = useAttachmentInsertStore.getState().consume(sessionId);
    if (items?.length) setPendingAttachments((prev) => [...prev, ...items]);
  }, [sessionId]);

  // 划词提问:消费预填文本(主窗口被唤起后新建会话跳转至此)
  useEffect(() => {
    if (!sessionId) return;
    const p = useSessionsStore.getState().pendingComposer;
    if (p && p.sessionId === sessionId) {
      setComposerText(p.text);
      useSessionsStore.getState().setPendingComposer(null);
    }
  }, [sessionId]);

  // ---------- B2:当前会话绑定的 Agent ----------
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);
  const agent = useMemo(
    () => (session?.agent_id ? agentTemplates.find((t) => t.agent_id === session.agent_id) ?? null : null),
    [session?.agent_id, agentTemplates],
  );

  // ---------- F4:会话分支树 ----------
  const [branches, setBranches] = useState<ipc.BranchInfo[]>([]);
  const [branchOpen, setBranchOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const activeBranch = session?.active_branch ?? 'main';
  const activeBranchInfo = useMemo(
    () => branches.find((b) => b.branch_id === activeBranch),
    [branches, activeBranch],
  );

  const loadBranches = useCallback(async () => {
    if (!sessionId) return;
    try {
      setBranches(await ipc.listBranches(sessionId));
    } catch {
      // 分支列表加载失败不阻塞对话
    }
  }, [sessionId]);

  /** 从目标消息处分叉(F4):后端切激活分支 → 刷新会话状态 → 强制重载消息与分支列表。
   *  旧分支完整保留,新分支可见历史 = 目标消息之前的前缀。 */
  const forkAndReload = useCallback(
    async (msgId: number) => {
      if (!sessionId) return;
      await ipc.forkBranch(sessionId, msgId);
      await useSessionsStore.getState().refresh();
      await load(sessionId, true);
      await loadBranches();
    },
    [sessionId, load, loadBranches],
  );

  /** 切换分支(store 更新 active_branch 后由 effect 统一重载)。 */
  const onSwitchBranch = async (bid: string) => {
    setBranchOpen(false);
    if (!sessionId || bid === activeBranch) return;
    try {
      await setActiveBranch(sessionId, bid);
    } catch (e) {
      setError(`切换分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const onRenameBranch = async () => {
    if (!sessionId || !renaming) return;
    try {
      await ipc.renameBranch(sessionId, renaming.id, renaming.name);
    } catch (e) {
      setError(`重命名分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setRenaming(null);
    await loadBranches();
  };

  /** 删除分支(「确认放弃整支」:UI 两态确认,后端级联清理自有消息与 FTS)。 */
  const onDeleteBranch = async (bid: string) => {
    if (!sessionId) return;
    try {
      await ipc.deleteBranch(sessionId, bid);
    } catch (e) {
      setError(`删除分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setDeleting(null);
    // 若删除的是激活分支,后端已回退 main;刷新会话状态触发消息重载
    await useSessionsStore.getState().refresh();
    await load(sessionId, true);
    await loadBranches();
  };

  // ---------- D3.1/D3.2:模型切换交接 ----------
  /** 检测到模型切换时弹出的交接信息 */
  const [handoff, setHandoff] = useState<{ from: string; to: string } | null>(null);
  /** 切换完成后的横幅(D3.2) */
  const [switchBanner, setSwitchBanner] = useState<{
    from: string;
    to: string;
    tokens: number;
    injected: string;
  } | null>(null);
  const [showInjected, setShowInjected] = useState(false);
  /** 记忆自动压缩提示(D3.5) */
  const [compressNotice, setCompressNotice] = useState<string | null>(null);
  const prevModelRef = useRef<string | null>(null);

  /** 解析会话记录的模型(provider/model;兼容旧的裸模型名)→ 首个可用目标(P1)。 */
  const resolveSessionModel = (modelId: string | null) => {
    if (!modelId) return null;
    const { templates, hasKey } = useProvidersStore.getState();
    const ready = templates.filter((t) => hasKey[t.id] && t.models.length > 0);
    const i = modelId.indexOf('/');
    if (i > 0) {
      const pid = modelId.slice(0, i);
      const m = modelId.slice(i + 1);
      const t = ready.find((x) => x.id === pid);
      return t && t.models.includes(m) ? { providerId: pid, model: m } : null;
    }
    const t = ready.find((x) => x.models.includes(modelId));
    return t ? { providerId: t.id, model: modelId } : null;
  };

  // 会话切换:重置交接弹窗与横幅
  useEffect(() => {
    setSwitchBanner(null);
    setHandoff(null);
  }, [sessionId]);

  // P1:进入会话时用会话记录的模型覆盖全局选择(每个对话可自定义模型),并同步交接基线
  useEffect(() => {
    const target = resolveSessionModel(session?.model_id ?? null);
    if (target) {
      prevModelRef.current = `${target.providerId}/${target.model}`;
      if (current?.providerId !== target.providerId || current?.model !== target.model) {
        selectModel(target.providerId, target.model);
      }
    } else {
      prevModelRef.current = current ? `${current.providerId}/${current.model}` : null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, session?.model_id]);

  /** 用户在弹窗中选定模型:显式触发切换与记忆交接(P1 显式化)。 */
  const onPickModel = (providerId: string, model: string) => {
    const from =
      prevModelRef.current ?? (current ? `${current.providerId}/${current.model}` : null);
    const to = `${providerId}/${model}`;
    selectModel(providerId, model);
    prevModelRef.current = to;
    if (from && from !== to && sessionId) setHandoff({ from, to });
  };

  /** 取消交接:把模型回滚到切换前,避免静默切换(P4)。 */
  const onCancelHandoff = () => {
    if (handoff) {
      const i = handoff.from.indexOf('/');
      if (i > 0) {
        prevModelRef.current = handoff.from;
        selectModel(handoff.from.slice(0, i), handoff.from.slice(i + 1));
      }
    }
    setHandoff(null);
  };

  /** 交接确认:快照 → 落盘策略 → 重读记忆 → 横幅展示注入统计(D3.1+D3.2)。 */
  const onHandoffConfirm = async (strategy: MemoryStrategy) => {
    if (!handoff || !sessionId || !current) return;
    const h = handoff;
    setHandoff(null);
    try {
      await ipc.snapshotMemory(sessionId, 'model_switch');
    } catch {
      // 快照失败不阻塞切换
    }
    await setMemoryStrategy(sessionId, strategy);
    await setSessionModel(sessionId, h.to);
    await loadMemory(sessionId);
    // 用新策略计算实际注入内容与 token 统计
    const mem = useMemoryStore.getState().bySession[sessionId];
    const injected = buildSystemWith(mem?.body ?? '', strategy);
    setSwitchBanner({ from: h.from, to: h.to, tokens: estimateTokens(injected), injected });
  };

  // 全局搜索跳转定位
  const [searchParams, setSearchParams] = useSearchParams();
  useEffect(() => {
    const target = searchParams.get('msg');
    if (!target || messages.length === 0) return;
    const el = document.getElementById(`msg-${target}`);
    if (!el) return;
    el.scrollIntoView({ block: 'center' });
    el.classList.add('msg-flash');
    const t = window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
    setSearchParams({}, { replace: true });
    return () => window.clearTimeout(t);
  }, [searchParams, messages.length, setSearchParams]);

  // 进入会话时加载消息和 memory
  useEffect(() => {
    if (!sessionId) return;
    load(sessionId);
    loadMemory(sessionId);
    // 离开会话时:空对话默认不保留(后端按真实消息数判断,流式中/已有内容的会话不受影响)
    return () => {
      void useSessionsStore.getState().removeIfEmpty(sessionId);
    };
  }, [sessionId, load, loadMemory]);

  // F4:激活分支变化(切换/分叉/搜索跳转)时强制重载消息与分支列表
  useEffect(() => {
    if (!sessionId) return;
    void load(sessionId, true);
    void loadBranches();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, activeBranch]);

  // 进入会话页加载 Agent 模板(B2)
  useEffect(() => {
    loadAgents();
  }, [loadAgents]);

  // 加载单模型配置(D8):交接弹窗按其默认记忆策略预选
  useEffect(() => {
    loadModelConfig().catch(() => {});
  }, [loadModelConfig]);

  // ---------- 思考能力(v1.9.0 工作块 I) ----------
  const [thinkCap, setThinkCap] = useState<ipc.ReasoningCapability | null>(null);
  const thinkingSel = useThinkingStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const initThinking = useThinkingStore((s) => s.init);
  const setThinking = useThinkingStore((s) => s.set);

  // 当前模型的思考能力(手动修正 > 内置规则表推断;查询失败按不支持处理,控件整体隐藏)
  useEffect(() => {
    let alive = true;
    if (!current) {
      setThinkCap(null);
      return;
    }
    ipc
      .getReasoningCapability(current.providerId, current.model)
      .then((cap) => alive && setThinkCap(cap))
      .catch(() => alive && setThinkCap(null));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.providerId, current?.model]);

  // 会话思考选择初始化:单模型默认档(default_reasoning)> 全局默认 medium;同会话切模型不覆盖已选
  useEffect(() => {
    if (!sessionId || !current) return;
    const def = modelConfig[current.providerId]?.[current.model]?.default_reasoning;
    const lv = (['light', 'medium', 'high', 'ultra'] as const).find((x) => x === def);
    initThinking(sessionId, lv);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, current?.providerId, current?.model, modelConfig]);

  // 首页草稿接续:从首页创建会话后,一次性消费并**直接发送**(失败回填输入框,内容不丢失)
  // 附件随草稿一并交接(首页无法在会话创建前落盘)
  const draftSentRef = useRef(false);
  useEffect(() => {
    if (draftSentRef.current) return;
    const d = sessionStorage.getItem('metaagent.draft');
    const rawAtt = sessionStorage.getItem('metaagent.draft.att');
    let draftAtt: { metas: AttachmentMeta[]; images: Record<string, string> } | undefined;
    if (rawAtt) {
      sessionStorage.removeItem('metaagent.draft.att');
      try {
        draftAtt = JSON.parse(rawAtt) as { metas: AttachmentMeta[]; images: Record<string, string> };
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

  // 新消息到达 / 切换会话时:平滑滚动到底部
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [sessionId, messages.length]);

  // 流式滚动跟随已移入 StreamingBubble 内部(随去抖刷新 auto 滚动)

  // ---------- 构建 system prompt ----------
  // 学习进度摘要(P2 记忆注入):进页取一次,失败静默(学习板块为空时后端返回 null)
  const [studyDigest, setStudyDigest] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    studyMemoryDigest()
      .then((d) => alive && setStudyDigest(d))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  /** 用指定记忆正文 + 策略组 system(交接确认时用最新值,不等 state 生效)。
   *  B4:sessions.system_prompt 是唯一基座事实源(绑定 Agent / 应用为提示词时
   *  后端已把渲染变量的 prompt 写入);agent 模板原文仅在旧数据(绑定但会话 prompt 为空)时兜底。 */
  const buildSystemWith = (memoryBody: string, strategy: MemoryStrategy): string => {
    const base =
      session?.system_prompt?.trim() ||
      agent?.system_prompt ||
      '你是 MAF Agent,一款专为学习、调研而创造的智能体框架软件。';
    const injected = injectByStrategy(memoryBody, strategy);
    // 学习进度块:与记忆正文并列追加,超预算时同样受护栏约束
    const studyBlock = studyDigest
      ? `\n\n以下是用户当前的学习进度(来自学习规划板块),回答学习相关问题时请参考:\n${studyDigest}`
      : '';
    if (!injected && !studyBlock) return base;
    if (!injected) return `${base}${studyBlock}`;
    // D3.5 注入预算护栏:超限硬截断兜底(正常路径已在发送前自动压缩)
    const memText = enforceBudget(injected);
    if (!studyBlock) return `${base}\n\n以下是关于当前会话的记忆(memory.md),请在回答时参考:\n\n${memText}`;
    return `${base}\n\n以下是关于当前会话的记忆(memory.md),请在回答时参考:\n\n${memText}${studyBlock}`;
  };

  const buildSystem = useCallback(
    (): string => buildSystemWith(memoryDto?.body ?? '', session?.memory_injection_strategy ?? 'full'),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [memoryDto?.body, session?.memory_injection_strategy, session?.system_prompt, agent, studyDigest],
  );

  // ---------- 公共流式发送 ----------
  /** modelOverride(F5 换模型重试):用指定模型跑本次流式,不动全局/会话模型,
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
        const { done, cancel } = ipc.chatStream(
          {
            provider_id: eff.providerId,
            model: eff.model,
            messages: history,
            temperature: null,
            max_tokens: null,
            top_p: null,
            custom_base_url: null,
            custom_display_name: null,
            session_id: sessionId,
            reasoning_effort: reasoningEffort,
          },
          (chunk) => {
            if (chunk.type === 'delta') {
              pushStreamDelta(sessionId, chunk.content);
            } else if (chunk.type === 'reasoning') {
              // 思考流分流:独立缓冲去抖,折叠块展示,不进正文
              pushStreamReasoning(sessionId, chunk.content);
            } else if (chunk.type === 'done') {
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
            sessionId,
            fullText,
            eff.model,
            tokens,
            sources && sources.length > 0 ? JSON.stringify(sources) : null,
            reasoning,
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
    [
      sessionId,
      current,
      agent,
      thinkCap,
      thinkingSel,
      startStreaming,
      pushStreamDelta,
      pushStreamReasoning,
      setStreamReasoningTokens,
      endStreaming,
      appendAssistant,
    ],
  );

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
    const quoted = content
      .split('\n')
      .map((l) => `> ${l}`)
      .join('\n');
    setComposerText((prev) => (prev ? `${quoted}\n\n${prev}` : `${quoted}\n\n`));
  }, []);

  const onEditResend = useCallback(
    async (msg: BubbleMessage) => {
      if (!sessionId) return;
      setComposerText(msg.content);
      const metas = parseAttachments(msg.attachments);
      setPendingAttachments(metas);
      // 图片附件重读 data URL 到 pendingImages
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
      setPendingImages(imgs);
      // F4:不再物理删除后续消息,从该消息处分叉(旧分支完整保留);
      // 用户随后发送的新消息自动落入新分支
      await forkAndReload(msg.id);
    },
    [sessionId, forkAndReload],
  );

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
      // 用新分支的最新历史重发(无新用户消息):system + 滑窗截断后的历史
      const latest = useMessagesStore.getState().bySession[sessionId] ?? [];
      const maxMsg = session?.max_context_messages ?? 0;
      let base = latest.slice();
      if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
      const lastAttachIdx = lastAttachIndex(base);
      const history = base.map((m, idx) => ({
        role: m.role,
        content: injectAttachments(m, idx === lastAttachIdx),
      }));
      await runStream([{ role: 'system', content: buildSystem() }, ...history]);
    },
    [sessionId, forkAndReload, session?.max_context_messages, buildSystem, runStream],
  );

  // ---------- F5:换模型重试 ----------
  const [retryTarget, setRetryTarget] = useState<BubbleMessage | null>(null);

  /** 选定模型后:从该条 AI 消息处分叉并以新模型重发(model_id 记新模型,不动会话模型;
   *  取消弹窗 = 仅 setRetryTarget(null),无副作用)。 */
  const onPickRetryModel = useCallback(
    async (providerId: string, model: string) => {
      const msg = retryTarget;
      setRetryTarget(null);
      if (!sessionId || !msg) return;
      // F4:分叉替代物理删除
      await forkAndReload(msg.id);
      const latest = useMessagesStore.getState().bySession[sessionId] ?? [];
      const maxMsg = session?.max_context_messages ?? 0;
      let base = latest.slice();
      if (maxMsg > 0 && base.length > maxMsg) base = base.slice(-maxMsg);
      const lastAttachIdx = lastAttachIndex(base);
      const history = base.map((m, idx) => ({
        role: m.role,
        content: injectAttachments(m, idx === lastAttachIdx),
      }));
      await runStream([{ role: 'system', content: buildSystem() }, ...history], null, {
        providerId,
        model,
      });
    },
    [sessionId, retryTarget, forkAndReload, session?.max_context_messages, buildSystem, runStream],
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

  // ---------- F8:一键钉入记忆(写入指定节,快照 note=pin_message) ----------
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2200);
  }, []);

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

  /** 最后一条 AI 消息 id(决定「重新生成」按钮归属);memo 避免每次渲染全量反扫。 */
  const lastAssistantId = useMemo(
    () => [...messages].reverse().find((m) => m.role === 'assistant')?.id,
    [messages],
  );

  if (!sessionId) return null;

  // ---------- D8:交接弹窗预选策略(目标模型的单模型配置 > 会话当前策略) ----------
  const handoffDefaultStrategy: MemoryStrategy = (() => {
    const fallback = session?.memory_injection_strategy ?? 'full';
    if (!handoff) return fallback;
    const i = handoff.to.indexOf('/');
    if (i < 0) return fallback;
    const pid = handoff.to.slice(0, i);
    const m = handoff.to.slice(i + 1);
    return modelConfig[pid]?.[m]?.memory_injection ?? fallback;
  })();

  // ---------- D3.5:记忆预算护栏(发送前检测,超限自动压缩) ----------
  const enforceMemoryBudget = async () => {
    if (!sessionId || !current) return;
    let st: ipc.BudgetStatus;
    try {
      st = await ipc.memoryBudgetStatus(sessionId);
    } catch {
      return;
    }
    if (!st.over) return;
    try {
      // 提炼专用小模型优先(D3.7)
      const settings = await ipc.getSettings().catch(() => null);
      const pid = settings?.summarize_provider_id || current.providerId;
      const m = settings?.summarize_model || current.model;
      const content = await ipc.summarizeMemory({
        session_id: sessionId,
        provider_id: pid,
        model: m,
        custom_base_url: null,
        custom_display_name: null,
      });
      await ipc.writeMemory(sessionId, content, 'auto_compress');
      await loadMemory(sessionId);
      const after = await ipc.memoryBudgetStatus(sessionId).catch(() => null);
      setCompressNotice(
        `记忆已自动压缩(约 ${st.tokens} → ${after?.tokens ?? '?'} tokens,上限 ${MEMORY_BUDGET_TOKENS})`,
      );
    } catch {
      setCompressNotice('记忆超出预算,自动压缩失败,本次注入将临时截断');
    }
  };

  // ---------- 构建带滑窗与附件注入的历史 ----------
  const buildHistory = (extraUser: { content: string; images?: string[] } | null) => {
    const maxMsg = session?.max_context_messages ?? 0;
    // 系统提示算 1 条;历史消息条数 = maxMsg (0=全部)
    let base = messages.slice();
    if (maxMsg > 0 && base.length > maxMsg) {
      base = base.slice(-maxMsg);
    }
    // 附件注入:仅范围内最后一条带附件的消息注入全文
    const lastAttachIdx = lastAttachIndex(base);
    const history = base.map((m, idx) => {
      const isLastAttach = idx === lastAttachIdx;
      return {
        role: m.role,
        content: injectAttachments(m, isLastAttach),
      };
    });
    const out: { role: string; content: string; images?: string[] }[] = [
      { role: 'system', content: buildSystem() },
      ...history,
    ];
    if (extraUser) {
      out.push({ role: 'user', content: extraUser.content, images: extraUser.images });
    }
    return out;
  };

  // ---------- onSend ----------
  /** overrideText:首页草稿直发用(绕过输入框状态);overrideAtt 同理携带首页已落盘的附件。
   *  发送失败时回填输入框。 */
  const onSend = async (
    overrideText?: string,
    overrideAtt?: { metas: AttachmentMeta[]; images: Record<string, string> },
  ) => {
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
      const userParts = text ? [resolveAtRefs(text)] : [];
      const images: string[] = [];
      for (const a of atts) {
        if (a.kind === 'image') {
          const du = imgs[a.path];
          if (du) images.push(du);
          userParts.push(`[图片附件:${a.name}]`);
        } else if (a.extracted_text) {
          userParts.push(`【附件:${a.name}】\n${a.extracted_text}`);
        } else {
          userParts.push(`[附件:${a.name}]`);
        }
      }
      const llmContentBase = userParts.join('\n\n');
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
            const list = res
              .map((s) => `[${s.id}] ${s.title}\n${s.url}\n${s.snippet}`)
              .join('\n\n');
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

  // ---------- 联网开关(会话级持久化) ----------
  const onToggleWebSearch = async () => {
    if (!session) return;
    await setWebSearch(session.id, !session.web_search_enabled);
  };

  // ---------- 附件处理 ----------
  const handlePickFiles = async (files: FileList) => {
    const picked = Array.from(files);
    const sizeErr = ipc.validateAttachmentFiles(picked, pendingAttachments.length);
    if (sizeErr) {
      setError(sizeErr);
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
            // 去掉 data:...;base64, 前缀
            const idx = r.indexOf(',');
            res(idx >= 0 ? r.slice(idx + 1) : r);
          };
          reader.onerror = rej;
          reader.readAsDataURL(f);
        });
        const meta = await ipc.saveAttachment(sessionId, f.name, b64);
        newMetas.push(meta);
        if (meta.kind === 'image') {
          // 前端保留完整 data URL 供 vision 注入,mime 按扩展名推断
          const ext = f.name.split('.').pop()?.toLowerCase() ?? 'png';
          const mime = ext === 'jpg' ? 'image/jpeg' : `image/${ext}`;
          newImages[meta.path] = `data:${mime};base64,${b64}`;
        }
      } catch (e) {
        setError(`附件 ${f.name} 上传失败: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
    if (newMetas.length) {
      setPendingAttachments((prev) => [...prev, ...newMetas]);
      setPendingImages(newImages);
    }
  };

  const handleRemoveAttachment = (index: number) => {
    setPendingAttachments((prev) => {
      const next = [...prev];
      const removed = next.splice(index, 1)[0];
      if (removed && removed.kind === 'image') {
        setPendingImages((im) => {
          const ni = { ...im };
          delete ni[removed.path];
          return ni;
        });
      }
      return next;
    });
  };

  return (
    <>
      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          {/* 会话头部:标题 + 右侧本会话模型选择(每个对话可自定义模型) */}
          <div className="chat-head">
            <span className="chat-title ellipsis">{session?.title ?? '对话'}</span>
            {/* F4:分支指示条(仅多分支或激活分支非 main 时显示) */}
            {(branches.length > 1 || activeBranch !== 'main') && (
              <div className="branch-chip-wrap">
                <button
                  type="button"
                  className="branch-chip"
                  title="会话分支:编辑重发 / 重新生成会产生新分支"
                  onClick={() => {
                    setRenaming(null);
                    setDeleting(null);
                    setBranchOpen((v) => !v);
                  }}
                >
                  <IconPullRequest size={13} />
                  <span>
                    {branchLabel(activeBranchInfo, activeBranch)} · 共 {branches.length} 分支
                  </span>
                  <IconChevronDown size={12} />
                </button>
                {branchOpen && (
                  <>
                    <div className="cmd-backdrop" onClick={() => setBranchOpen(false)} />
                    <div className="branch-pop">
                      {branches.map((b) => (
                        <div
                          key={b.branch_id}
                          className={`branch-item${b.branch_id === activeBranch ? ' active' : ''}`}
                        >
                          {renaming?.id === b.branch_id ? (
                            <div className="branch-rename-row">
                              <input
                                autoFocus
                                value={renaming.name}
                                placeholder={b.branch_id === 'main' ? '主分支' : `分支 ${b.branch_id.slice(0, 4)}`}
                                onChange={(e) => setRenaming({ id: b.branch_id, name: e.target.value })}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') void onRenameBranch();
                                  if (e.key === 'Escape') setRenaming(null);
                                }}
                              />
                              <button type="button" title="确认" onClick={() => void onRenameBranch()}>
                                <IconCheck size={14} />
                              </button>
                              <button type="button" title="取消" onClick={() => setRenaming(null)}>
                                <IconX size={14} />
                              </button>
                            </div>
                          ) : (
                            <>
                              <div
                                className="branch-item-main"
                                onClick={() => void onSwitchBranch(b.branch_id)}
                              >
                                <div className="branch-item-name">
                                  {branchLabel(b, b.branch_id)}
                                  {b.branch_id === activeBranch && <span className="tag">当前</span>}
                                </div>
                                <div className="branch-item-sub">
                                  {b.first_summary || '(空分支)'} · {b.message_count} 条
                                </div>
                              </div>
                              <button
                                type="button"
                                title="重命名"
                                onClick={() => setRenaming({ id: b.branch_id, name: b.name })}
                              >
                                <IconEdit size={13} />
                              </button>
                              {b.branch_id !== 'main' &&
                                (deleting === b.branch_id ? (
                                  <button
                                    type="button"
                                    className="branch-del-confirm"
                                    title="确认删除该分支及其全部消息"
                                    onClick={() => void onDeleteBranch(b.branch_id)}
                                  >
                                    确认
                                  </button>
                                ) : (
                                  <button
                                    type="button"
                                    title="删除分支"
                                    onClick={() => setDeleting(b.branch_id)}
                                  >
                                    <IconTrash size={13} />
                                  </button>
                                ))}
                            </>
                          )}
                        </div>
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}
            <button
              type="button"
              className="model-switch"
              onClick={() => setModelSwitchOpen(true)}
              title="切换本会话使用的模型"
            >
              <span className={`stat-dot ${current ? 'ok' : 'warn'}`} />
              {current ? current.model : '未配置模型'}
              {current && <span className="tag">{current.providerId}</span>}
              <IconChevronDown size={13} />
            </button>
            <button
              type="button"
              className={`chat-panel-toggle${memPanel.collapsed ? '' : ' on'}`}
              onClick={memPanel.toggleCollapse}
              title={memPanel.collapsed ? '展开记忆面板' : '收起记忆面板'}
            >
              <IconBrain size={15} />
            </button>
          </div>
          <div className="chat-scroll" ref={scrollRef}>
            <div className="chat-inner">
              {switchBanner && (
                <div className="switch-banner">
                  <span className="switch-banner-text">
                    已从 {switchBanner.from} 切换到 {switchBanner.to} · 记忆已注入约{' '}
                    {switchBanner.tokens} tokens
                  </span>
                  <button
                    type="button"
                    className="switch-banner-btn"
                    onClick={() => setShowInjected(true)}
                  >
                    查看注入内容
                  </button>
                  <button
                    type="button"
                    className="switch-banner-close"
                    title="关闭"
                    onClick={() => setSwitchBanner(null)}
                  >
                    <IconX size={14} />
                  </button>
                </div>
              )}
              {compressNotice && (
                <div className="switch-banner compress">
                  <span className="switch-banner-text">{compressNotice}</span>
                  <button
                    type="button"
                    className="switch-banner-close"
                    title="关闭"
                    onClick={() => setCompressNotice(null)}
                  >
                    <IconX size={14} />
                  </button>
                </div>
              )}
              {messages.length === 0 && !hasStreaming && (
                <div className="main-empty">
                  <div style={{ color: 'var(--text-4)' }}>
                    <IconChat size={48} strokeWidth={1} />
                  </div>
                  <div>开始你的第一段对话吧</div>
                </div>
              )}
              <MessageList
                messages={messages}
                lastAssistantId={lastAssistantId}
                onQuote={onQuote}
                onTranslate={onTranslate}
                onEditResend={onEditResend}
                onDelete={onDelete}
                onRegenerate={onRegenerate}
                onRetryModel={setRetryTarget}
                onSaveSources={onSaveSources}
                onPin={onPin}
              />
              {hasStreaming && sessionId && (
                <StreamingBubble sessionId={sessionId} modelId={current?.model ?? null} />
              )}
              {error && (
                <div
                  style={{
                    padding: 'var(--space-3)',
                    background: 'rgba(201,59,59,.08)',
                    border: '1px solid rgba(201,59,59,.3)',
                    borderRadius: 'var(--radius-m)',
                    color: 'var(--err)',
                    fontSize: 'var(--font-sub)',
                  }}
                >
                  {error}
                </div>
              )}
            </div>
          </div>
          <Composer
            value={composerText}
            onChange={setComposerText}
            onSend={onSend}
            disabled={sending || searching}
            variant="docked"
            sending={sending}
            onStop={onCancel}
            placeholder={
              searching
                ? '正在联网搜索…'
                : sending
                  ? 'AI 正在回复…'
                  : agent?.input_hints?.[0]
            }
            attachments={pendingAttachments}
            onPickFiles={handlePickFiles}
            onRemoveAttachment={handleRemoveAttachment}
            onRecall={onRecall}
            webSearch={session?.web_search_enabled ?? false}
            onToggleWebSearch={onToggleWebSearch}
            thinking={
              thinkCap?.enabled && current
                ? {
                    capability: thinkCap,
                    providerId: current.providerId,
                    model: current.model,
                    selection: thinkingSel ?? { on: true, level: DEFAULT_THINKING_LEVEL },
                    onSelect: (sel) => {
                      if (sessionId) setThinking(sessionId, sel);
                    },
                  }
                : null
            }
          >
            {/* B2:当前 Agent 指示,点击切换/解绑 */}
            <button
              type="button"
              className="tool-chip agent-chip-btn"
              title={agent ? `当前智能体:${agent.name}(点击切换)` : '点击选择智能体'}
              onClick={() => setAgentPickerOpen(true)}
            >
              {(() => {
                const Icon = agentIconComponent(agent?.icon ?? 'bot');
                return <Icon size={13} />;
              })()}
              <span className="agent-chip-text">{agent?.name ?? '智能体'}</span>
            </button>
          </Composer>
        </div>
        {!memPanel.collapsed && (
          <>
            <ResizeHandle panel={memPanel} />
            <MemPanel
              sessionId={sessionId}
              width={memPanel.width}
              narrow={memPanel.narrow}
              onCollapse={memPanel.toggleCollapse}
            />
          </>
        )}
      </div>

      {modelSwitchOpen && (
        <ModelSwitchDialog onClose={() => setModelSwitchOpen(false)} onPick={onPickModel} />
      )}

      {/* F5:换模型重试(取消无副作用) */}
      {retryTarget && (
        <ModelSwitchDialog onClose={() => setRetryTarget(null)} onPick={onPickRetryModel} />
      )}

      {agentPickerOpen && (
        <AgentPickerDialog
          currentAgentId={session?.agent_id ?? null}
          sessionId={sessionId}
          onSelect={async (agentId) => {
            setAgentPickerOpen(false);
            if (!sessionId) return;
            try {
              await setSessionAgent(sessionId, agentId);
            } catch (e) {
              setError(`切换智能体失败:${e instanceof Error ? e.message : String(e)}`);
            }
          }}
          onClose={() => setAgentPickerOpen(false)}
        />
      )}

      {handoff && (
        <MemoryHandoffDialog
          from={handoff.from}
          to={handoff.to}
          memoryBody={memoryDto?.body ?? ''}
          currentStrategy={handoffDefaultStrategy}
          onConfirm={onHandoffConfirm}
          onCancel={onCancelHandoff}
        />
      )}

      {showInjected && switchBanner && (
        <div className="dialog-mask" onClick={() => setShowInjected(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ width: 560 }}>
            <div className="dialog-head">实际注入的 system 文本</div>
            <div className="dialog-body">
              <pre className="injected-view">{switchBanner.injected}</pre>
            </div>
          </div>
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}
    </>
  );
}
