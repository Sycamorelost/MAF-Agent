import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { mcpClearSessionAllowed } from '../../lib/ipc/mcp';
import StreamingBubble from '../../components/StreamingBubble';
import Composer from '../../components/Composer';
import { ResizeHandle } from '../../components/ResizeHandle';
import { usePanelWidth } from '../../lib/usePanelWidth';
import MemPanel from '../../components/MemPanel';
import MemoryHandoffDialog from '../../components/MemoryHandoffDialog';
import ModelSwitchDialog from '../../components/ModelSwitchDialog';
import AgentPickerDialog from '../../components/AgentPickerDialog';
import OutlinePanel from '../../components/OutlinePanel';
import { IconChat, IconX, agentIconComponent } from '../../components/icons';
import { PermissionChip } from '../../components/PermissionChip';
import { resolveMode, useDefaultWorkbench } from '../../lib/workbench';
import { useWorkbenchChrome } from '../../lib/workbenchChrome';
import { memoryBudgetStatus } from '../../lib/ipc/memory';
import { useMessagesStore } from '../../stores/messages';
import { useSessionsStore } from '../../stores/sessions';
import { useProvidersStore } from '../../stores/providers';
import { useMemoryStore } from '../../stores/memory';
import { useAgentsStore } from '../../stores/agents';
import { useThinkingStore, DEFAULT_THINKING_LEVEL, agentThinking } from '../../stores/thinking';
import type { AttachmentMeta, MessageRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { MessageList } from './MessageList';
import { ChatHeader } from './ChatHeader';
import { useModelHandoff } from './useModelHandoff';
import { ChatTodos } from './ChatTodos';
import { useChatSend } from './useChatSend';
import { useDraftHandoff } from './useDraftHandoff';

/** 稳定空数组:避免 `?? []` 每次渲染生成新引用导致 MessageList memo 失效。 */
const EMPTY_MESSAGES: MessageRow[] = [];
/** 长会话渲染窗口(P2 性能):默认只挂载最近 N 条,更早的分页/展开按需加载,控制 DOM 体量。 */
const RENDER_WINDOW = 120;

export default function ChatPage() {
  const { id: sessionId } = useParams<{ id: string }>();
  const memPanel = usePanelWidth('mempanel', 320, 240, 520, 'right');
  const { bySession, load } = useMessagesStore();
  const hasMore = useMessagesStore((s) => (sessionId ? s.hasMore[sessionId] === true : false));
  const loadOlder = useMessagesStore((s) => s.loadOlder);
  // 流式状态用 boolean selector:只在开始/结束变化两次,ChatPage 不随 50ms 去抖重渲染
  const hasStreaming = useMessagesStore((s) =>
    sessionId ? s.streaming[sessionId] !== undefined : false,
  );
  const sessions = useSessionsStore((s) => s.sessions);
  const setSessionAgent = useSessionsStore((s) => s.setAgent);
  const current = useProvidersStore((s) => s.current);
  // 单模型配置(迭代 D8):思考默认档与交接预选策略
  const modelConfig = useProvidersStore((s) => s.modelConfig);
  const loadModelConfig = useProvidersStore((s) => s.loadModelConfig);
  const memoryDto = useMemoryStore((s) => (sessionId ? s.bySession[sessionId] : undefined));
  const loadMemory = useMemoryStore((s) => s.load);
  const { templates: agentTemplates, load: loadAgents } = useAgentsStore();
  const initThinking = useThinkingStore((s) => s.init);
  const setThinking = useThinkingStore((s) => s.set);
  const thinkingSel = useThinkingStore((s) => (sessionId ? s.bySession[sessionId] : undefined));

  const [composerText, setComposerText] = useState('');
  const [pendingAttachments, setPendingAttachments] = useState<AttachmentMeta[]>([]);
  const [pendingImages, setPendingImages] = useState<Record<string, string>>({});
  const [modelSwitchOpen, setModelSwitchOpen] = useState(false);
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const session = sessions.find((s) => s.id === sessionId);
  const messages = sessionId ? (bySession[sessionId] ?? EMPTY_MESSAGES) : EMPTY_MESSAGES;

  // ---------- D16 / val.md §5.8:工作台模式下,预设/权限 chip 落在 Composer 工具条内 ----------
  const defaultWb = useDefaultWorkbench();
  const isWork = resolveMode(session?.workbench_mode, defaultWb ?? 'chat') === 'work';
  // 工作台外壳注入的运行状态条:插在会话头之下、消息区之上(ui-design b3)
  const chrome = useWorkbenchChrome();

  // ---------- P2:长会话渲染窗口(默认只渲染最近 RENDER_WINDOW 条,可手动展开全部) ----------
  const [expandAll, setExpandAll] = useState(false);
  useEffect(() => setExpandAll(false), [sessionId]);
  const folded = !expandAll && messages.length > RENDER_WINDOW;
  const visibleMessages = folded ? messages.slice(-RENDER_WINDOW) : messages;

  // ---------- b3:记忆预算(自动压缩口径)用于 Composer hint 与预算护栏 ----------
  const [memBudget, setMemBudget] = useState<{ tokens: number; limit: number } | null>(null);
  useEffect(() => {
    if (!sessionId) return;
    memoryBudgetStatus(sessionId)
      .then((b) => setMemBudget({ tokens: b.tokens, limit: b.limit }))
      .catch(() => {});
  }, [sessionId, hasStreaming, messages.length]);

  /** 前插历史标记:加载更早一页时抑制「自动滚到底部」,避免把视口甩走 */
  const prependingRef = useRef(false);

  /** 加载更早一页并保持视口位置(内容前插后补偿滚动高度差) */
  const onLoadOlder = async () => {
    if (!sessionId) return;
    const sc = scrollRef.current;
    const before = sc ? sc.scrollHeight : 0;
    prependingRef.current = true;
    await loadOlder(sessionId);
    requestAnimationFrame(() => {
      if (sc) sc.scrollTop += sc.scrollHeight - before;
    });
  };
  // ---------- B2:当前会话绑定的 Agent ----------
  const agent = useMemo(
    () => (session?.agent_id ? agentTemplates.find((t) => t.agent_id === session.agent_id) ?? null : null),
    [session?.agent_id, agentTemplates],
  );

  // ---------- F4:会话分支 ----------
  /** 递增信号:同分支内分叉时也触发头部重载分支列表 */
  const [branchRefresh, setBranchRefresh] = useState(0);
  /** 从目标消息处分叉(F4):后端切激活分支 → 刷新会话状态 → 强制重载消息与分支列表。
   *  旧分支完整保留,新分支可见历史 = 目标消息之前的前缀。 */
  const forkAndReload = useCallback(
    async (msgId: number) => {
      if (!sessionId) return;
      await ipc.forkBranch(sessionId, msgId);
      await useSessionsStore.getState().refresh();
      await load(sessionId, true);
      setBranchRefresh((v) => v + 1);
    },
    [sessionId, load],
  );

  // 发送/流式/消息操作/记忆操作
  const chat = useChatSend({
    sessionId,
    session,
    current,
    agent,
    messages,
    memoryBody: memoryDto?.body ?? '',
    composerText,
    setComposerText,
    pendingAttachments,
    pendingImages,
    setPendingAttachments,
    setPendingImages,
    forkAndReload,
  });
  const activeBranch = session?.active_branch ?? 'main';

  // 全局搜索跳转定位
  const [searchParams, setSearchParams] = useSearchParams();
  const locatingRef = useRef(false);
  useEffect(() => {
    locatingRef.current = false;
  }, [sessionId, searchParams]);
  useEffect(() => {
    const target = searchParams.get('msg');
    if (!target || messages.length === 0) return;
    const el = document.getElementById(`msg-${target}`);
    if (!el) {
      // 目标消息在折叠窗口外:先展开全部再定位
      if (folded) {
        setExpandAll(true);
        return;
      }
      // 目标消息在更早分页里:继续向前加载,直到命中(或已无更早消息)
      if (hasMore && sessionId) {
        if (!locatingRef.current) {
          locatingRef.current = true;
          void loadOlder(sessionId);
        }
        return;
      }
      // 目标消息不存在(已被删除):清掉定位参数
      setSearchParams({}, { replace: true });
      return;
    }
    el.scrollIntoView({ block: 'center' });
    el.classList.add('msg-flash');
    const t = window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
    setSearchParams({}, { replace: true });
    return () => window.clearTimeout(t);
  }, [searchParams, messages.length, setSearchParams, folded, hasMore, sessionId, loadOlder]);

  // 进入会话时加载消息和 memory
  useEffect(() => {
    if (!sessionId) return;
    load(sessionId);
    loadMemory(sessionId);
    // 离开会话时:空对话默认不保留(后端按真实消息数判断,流式中/已有内容的会话不受影响)
    // 同时清空 MCP「始终允许」会话缓存,避免跨会话静默放行(Phase 5e 安全要求)
    return () => {
      void useSessionsStore.getState().removeIfEmpty(sessionId);
      void mcpClearSessionAllowed().catch(() => {});
    };
  }, [sessionId, load, loadMemory]);

  // F4:激活分支变化(切换/分叉/搜索跳转)时强制重载消息与分支列表
  useEffect(() => {
    if (!sessionId) return;
    void load(sessionId, true);
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

  // 会话思考选择初始化:智能体偏好 > 单模型默认档(default_reasoning)> 全局默认 medium;
  // 同会话切模型不覆盖已选(store 内 init 幂等)
  useEffect(() => {
    if (!sessionId || !current) return;
    // 等智能体模板加载完再初始化,避免模型默认档抢先占位
    if (session?.agent_id && agentTemplates.length === 0) return;
    const def = modelConfig[current.providerId]?.[current.model]?.default_reasoning;
    const lv = (['light', 'medium', 'high', 'ultra'] as const).find((x) => x === def);
    const pref = agentThinking(agent?.thinking_level);
    initThinking(sessionId, pref?.level ?? lv, pref?.on);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, current?.providerId, current?.model, modelConfig, agent?.thinking_level, agentTemplates.length, session?.agent_id]);

  // F7 附件插入队列 / 划词预填 / 首页草稿接续 / 附件增删
  const { handlePickFiles, handleRemoveAttachment } = useDraftHandoff({
    sessionId,
    setComposerText,
    pendingAttachments,
    pendingImages,
    setPendingAttachments,
    setPendingImages,
    onSend: chat.onSend,
    onError: chat.setError,
  });

  // 新消息到达 / 切换会话时:平滑滚动到底部
  // (?msg 定位期间不抢滚动,否则会把 scrollIntoView 的目标甩到底部;
  //  用 ref 读取最新目标,避免定位完成后清空 query 触发本 effect 再次拉到底部)
  const msgTargetRef = useRef<string | null>(null);
  msgTargetRef.current = searchParams.get('msg');
  useEffect(() => {
    if (prependingRef.current) {
      prependingRef.current = false;
      return;
    }
    if (msgTargetRef.current) return;
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [sessionId, messages.length]);

  // ---------- D3.1/D3.2:模型切换交接 ----------
  const { handoff, switchBanner, setSwitchBanner, showInjected, setShowInjected,
    handoffDefaultStrategy, onPickModel, onCancelHandoff, onHandoffConfirm } = useModelHandoff({
    sessionId,
    session,
    current,
    buildSystemWith: chat.buildSystemWith,
  });

  /** 最后一条 AI 消息 id(决定「重新生成」按钮归属);memo 避免每次渲染全量反扫。 */
  const lastAssistantId = useMemo(
    () => [...messages].reverse().find((m) => m.role === 'assistant')?.id,
    [messages],
  );

  if (!sessionId) return null;

  return (
    <>
      {/* 作为 .wb-main / .main 的 flex 项:必须 minWidth:0,否则长代码行等内容的最小内容宽度会撑破中栏 */}
      <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          {/* 会话头部:标题 + 右侧本会话模型选择(每个对话可自定义模型) */}
          <ChatHeader
            title={session?.title ?? '对话'}
            sessionId={sessionId}
            mode={isWork ? 'work' : 'chat'}
            activeBranch={activeBranch}
            branchRefresh={branchRefresh}
            current={current}
            memCollapsed={memPanel.collapsed}
            outlineOpen={outlineOpen}
            onToggleMemPanel={memPanel.toggleCollapse}
            onToggleOutline={() => setOutlineOpen((v) => !v)}
            onOpenModelSwitch={() => setModelSwitchOpen(true)}
            onError={chat.setError}
          />
          {chrome?.runBar}
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
              {chat.compressNotice && (
                <div className="switch-banner compress">
                  <span className="switch-banner-text">{chat.compressNotice}</span>
                  <button
                    type="button"
                    className="switch-banner-close"
                    title="关闭"
                    onClick={() => chat.setCompressNotice(null)}
                  >
                    <IconX size={14} />
                  </button>
                </div>
              )}
              {/* 工作台模式下任务清单已由左栏任务计划展示,列表隐藏但仍转发后端通知 */}
              <ChatTodos
                sessionId={sessionId}
                onNotice={chat.setCompressNotice}
                hidden={isWork}
              />
              {messages.length === 0 && !hasStreaming && (
                <div className="main-empty">
                  <div style={{ color: 'var(--text-4)' }}>
                    <IconChat size={48} strokeWidth={1} />
                  </div>
                  <div>{agent?.opening_message?.trim() || '开始你的第一段对话吧'}</div>
                  {!!agent?.suggestions?.length && (
                    <div className="main-empty-sugs">
                      {agent.suggestions.map((s) => (
                        <button
                          key={s}
                          type="button"
                          className="main-empty-sug"
                          onClick={() => setComposerText(s)}
                        >
                          {s}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              )}
              {hasMore && messages.length > 0 && (
                <div className="load-older">
                  <button type="button" className="load-older-btn" onClick={onLoadOlder}>
                    加载更早消息
                  </button>
                </div>
              )}
              {folded && (
                <div className="load-older">
                  <button
                    type="button"
                    className="load-older-btn"
                    onClick={() => setExpandAll(true)}
                  >
                    已折叠 {messages.length - RENDER_WINDOW} 条(性能保护),点击展开全部
                  </button>
                </div>
              )}
              <MessageList
                messages={visibleMessages}
                lastAssistantId={lastAssistantId}
                onQuote={chat.onQuote}
                onTranslate={chat.onTranslate}
                onEditResend={chat.onEditResend}
                onDelete={chat.onDelete}
                onRegenerate={chat.onRegenerate}
                onRetryModel={chat.setRetryTarget}
                onSaveSources={chat.onSaveSources}
                onPin={chat.onPin}
              />
              {hasStreaming && sessionId && (
                <StreamingBubble sessionId={sessionId} modelId={current?.model ?? null} />
              )}
              {chat.error && (
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
                  {chat.error}
                </div>
              )}
            </div>
          </div>
          <Composer
            value={composerText}
            onChange={setComposerText}
            onSend={chat.onSend}
            disabled={chat.sending || chat.searching}
            variant="docked"
            sending={chat.sending}
            onStop={chat.onCancel}
            placeholder={
              chat.searching
                ? '正在联网搜索…'
                : chat.sending
                  ? 'AI 正在回复…'
                  : agent?.input_hints?.[0]
            }
            attachments={pendingAttachments}
            onPickFiles={handlePickFiles}
            onRemoveAttachment={handleRemoveAttachment}
            onRecall={chat.onRecall}
            webSearch={session?.web_search_enabled ?? false}
            onToggleWebSearch={chat.onToggleWebSearch}
            compressInfo={memBudget}
            thinking={
              chat.thinkCap?.enabled && current
                ? {
                    capability: chat.thinkCap,
                    providerId: current.providerId,
                    model: current.model,
                    selection: thinkingSel ?? { on: false, level: DEFAULT_THINKING_LEVEL },
                    onSelect: (sel) => {
                      if (sessionId) setThinking(sessionId, sel);
                    },
                  }
                : null
            }
          >
            {/* D16 / §5.8:工作台模式下权限 chip 落在 composer-bar 内(靠近输入区);
                预设 chip 已在会话头(ui-design b3),此处不重复 */}
            {isWork && <PermissionChip sessionId={sessionId} onError={chat.setError} />}
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
        {!isWork && !memPanel.collapsed && !outlineOpen && (
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
        {!isWork && outlineOpen && (
          <OutlinePanel messages={messages} onClose={() => setOutlineOpen(false)} />
        )}
      </div>

      {modelSwitchOpen && (
        <ModelSwitchDialog onClose={() => setModelSwitchOpen(false)} onPick={onPickModel} />
      )}

      {/* F5:换模型重试(取消无副作用) */}
      {chat.retryTarget && (
        <ModelSwitchDialog onClose={() => chat.setRetryTarget(null)} onPick={chat.onPickRetryModel} />
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
              chat.setError(`切换智能体失败:${e instanceof Error ? e.message : String(e)}`);
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

      {chat.toast && <div className="toast">{chat.toast}</div>}
    </>
  );
}
