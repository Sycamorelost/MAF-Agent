import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import StreamingBubble from '../../components/StreamingBubble';
import OutlinePanel from '../../components/OutlinePanel';
import { Inspector } from '../../components/inspector/Inspector';
import { WorkbenchChromeContext } from '../../lib/workbenchChrome';

import { useMessagesStore } from '../../stores/messages';
import { useSessionsStore } from '../../stores/sessions';
import { useProvidersStore } from '../../stores/providers';
import { useMemoryStore } from '../../stores/memory';
import { useDeliverablesStore } from '../../stores/deliverables';
import { useAgentsStore } from '../../stores/agents';
import { useThinkingStore } from '../../stores/thinking';
import type { AttachmentMeta, MessageRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { buildQuote, type QuoteRef } from '../../lib/quote';
import { MessageList } from './MessageList';
import { ChatHeader } from './ChatHeader';
import { useModelHandoff } from './useModelHandoff';
import { ChatTodos } from './ChatTodos';
import { useChatSend } from './useChatSend';
import { resumeChatRun } from './chatRuntime';
import { useDraftHandoff } from './useDraftHandoff';
import { useChatInspector } from './useChatInspector';
import { ChatBanners } from './ChatBanners';
import { ChatEmptyState } from './ChatEmptyState';
import { ChatComposerArea } from './ChatComposerArea';
import { ChatDialogs } from './ChatDialogs';
import { useChatPageScroll } from './useChatPageScroll';
import { useChatSelectionToolbar, type ChatSelectionTool } from './useChatSelectionToolbar';
import { ChatSelectionToolbar } from './ChatSelectionToolbar';
import { useChatSessionEffects } from './useChatSessionEffects';

/** 稳定空数组:避免 `?? []` 每次渲染生成新引用导致 MessageList memo 失效。 */
const EMPTY_MESSAGES: MessageRow[] = [];

export default function ChatPage({ compactTools = false }: { compactTools?: boolean }) {
  const { id: sessionId } = useParams<{ id: string }>();
  const { bySession, load } = useMessagesStore();
  /** P15:引用卡片映射(按消息 id 查 quotes) */
  const quotesByMessageId = useMessagesStore((s) => s.quotesByMessageId);
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
  const loadDeliverables = useDeliverablesStore((s) => s.load);
  const { templates: agentTemplates, load: loadAgents } = useAgentsStore();
  const initThinking = useThinkingStore((s) => s.init);
  const setThinking = useThinkingStore((s) => s.set);
  const thinkingSel = useThinkingStore((s) => (sessionId ? s.bySession[sessionId] : undefined));

  const [composerText, setComposerText] = useState('');
  const [pendingAttachments, setPendingAttachments] = useState<AttachmentMeta[]>([]);
  const [pendingImages, setPendingImages] = useState<Record<string, string>>({});
  /** 引用条:完整原文留在 state,输入框只显示摘要;发送时作为独立上下文注入 */
  const [quotes, setQuotes] = useState<QuoteRef[]>([]);
  /** 引用消息(选区优先):最多保留 3 条,超出替换最早一条 */
  const handleQuote = useCallback(
    (content: string, meta?: { role?: string; createdAt?: string }) => {
      const selected = typeof window !== 'undefined' ? window.getSelection()?.toString().trim() : '';
      const text = selected && selected.length > 0 ? selected : content;
      setQuotes((prev) => [...prev, buildQuote(text, meta?.role ?? 'assistant', meta?.createdAt)].slice(-3));
    },
    [],
  );
  const [modelSwitchOpen, setModelSwitchOpen] = useState(false);
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);

  // 已归档会话同样可打开查看/继续对话,查找时需覆盖归档列表
  const session =
    sessions.find((s) => s.id === sessionId) ??
    useSessionsStore.getState().archived.find((s) => s.id === sessionId);
  const messages = sessionId ? (bySession[sessionId] ?? EMPTY_MESSAGES) : EMPTY_MESSAGES;

  // ---------- 检查器(P3-1:工作台能力永久开启,检查器默认开) ----------
  const { chatInspectorOpen, toggleChatInspector, chromeValue } = useChatInspector();

  // ---------- 滚动/定位与划词工具栏 ----------
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    scrollRef, listRef, listWrapRef, listMargin,
    onLoadOlder, onScrollFollow, onOutlineJump, resumeFollow,
  } = useChatPageScroll({ sessionId, messages, hasMore, loadOlder, searchParams, setSearchParams });
  const { selTool, setSelTool } = useChatSelectionToolbar(listWrapRef);

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
    quotes,
    setQuotes,
    forkAndReload,
  });
  const activeBranch = session?.active_branch ?? 'main';

  // 刷新/重进页面时恢复仍在进行的生成(sessionStorage 记录 → chat_run_state 重放)
  useEffect(() => {
    if (sessionId) void resumeChatRun(sessionId);
  }, [sessionId]);

  // 会话数据加载、分支重载、智能体模板/模型配置加载与思考档初始化
  useChatSessionEffects({
    sessionId, session, current, agent, agentTemplates, modelConfig, activeBranch,
    load, loadMemory, loadDeliverables, loadAgents, loadModelConfig, initThinking,
  });

  // F7 附件插入队列 / 预填文本 / 首页草稿接续 / 附件增删
  const { handlePickFiles, handleRemoveAttachment } = useDraftHandoff({
    sessionId,
    setComposerText,
    pendingAttachments,
    pendingImages,
    setPendingAttachments,
    setPendingImages,
    onSend: chat.onSend,
    onError: chat.setError,
    onNotice: (msg: string) => chat.showToast(msg, 5200),
  });

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

  /** 划词「追问」:用来源消息的原始 Markdown content 构建引用,
   *  而不是选区渲染后纯文本(公式等会丢失定界符)。 */
  const handleSelectionQuote = useCallback(
    (selection: ChatSelectionTool) => {
      const srcMsg = selection.msgId != null
        ? messages.find((m) => m.id === selection.msgId)
        : undefined;
      const quoteText = srcMsg?.content?.trim() || selection.text;
      const quoteRole = srcMsg?.role ?? 'assistant';
      setQuotes((prev) => [
        ...prev,
        buildQuote(quoteText, quoteRole, srcMsg?.created),
      ].slice(-3));
      setSelTool(null);
    },
    [messages, setSelTool],
  );

  if (!sessionId) return null;

  return (
    <WorkbenchChromeContext.Provider value={chromeValue}>
      {/* 作为 .wb-main / .main 的 flex 项:必须 minWidth:0,否则长代码行等内容的最小内容宽度会撑破中栏 */}
      <div className="chat-root">
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            minWidth: 0,
            position: 'relative',
            // 纲要横线轨的容器查询上下文:列宽不足时由 CSS 隐藏,避免污染正文
            containerType: 'inline-size',
          }}
        >
          {/* 会话头部:标题 + 右侧本会话模型选择(每个对话可自定义模型) */}
          {/* 小窗紧凑态不渲染会话头(标题/分支/导出/检查器等工具条整组移除) */}
          {!compactTools && (
            <ChatHeader
              title={session?.title ?? '对话'}
              sessionId={sessionId}
              activeBranch={activeBranch}
              branchRefresh={branchRefresh}
              onError={chat.setError}
            />
          )}
          <div className="chat-scroll" ref={scrollRef} onScroll={onScrollFollow}>
            <div className="chat-inner">
              <ChatBanners
                switchBanner={switchBanner}
                compressNotice={chat.compressNotice}
                onShowInjected={() => setShowInjected(true)}
                onCloseSwitch={() => setSwitchBanner(null)}
                onCloseCompress={() => chat.setCompressNotice(null)}
              />
              {messages.length === 0 && !hasStreaming && (
                <ChatEmptyState agent={agent} onPickSuggestion={setComposerText} />
              )}
              {hasMore && messages.length > 0 && (
                <div className="load-older">
                  <button type="button" className="load-older-btn" onClick={onLoadOlder}>
                    加载更早消息
                  </button>
                </div>
              )}
              <div ref={listWrapRef}>
                <MessageList
                  messages={messages}
                  quotesByMessageId={quotesByMessageId}
                  scrollRef={scrollRef}
                  listRef={listRef}
                  startMargin={listMargin}
                  lastAssistantId={lastAssistantId}
                  onQuote={handleQuote}
                  onEditResend={chat.onEditResend}
                  onDelete={chat.onDelete}
                  onRegenerate={chat.onRegenerate}
                  onRetryModel={chat.setRetryTarget}
                  onSaveSources={chat.onSaveSources}
                  onPin={chat.onPin}
                />
              </div>
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
          {/* 任务清单:固定在输入框(Composer)正上方,与输入框同高,实时更新 */}
          <ChatTodos sessionId={sessionId} onNotice={chat.setCompressNotice} />
          <ChatComposerArea
            sessionId={sessionId}
            session={session}
            current={current}
            agent={agent}
            compactTools={compactTools}
            composerText={composerText}
            setComposerText={setComposerText}
            pendingAttachments={pendingAttachments}
            setPendingAttachments={setPendingAttachments}
            setPendingImages={setPendingImages}
            quotes={quotes}
            setQuotes={setQuotes}
            onPickFiles={handlePickFiles}
            onRemoveAttachment={handleRemoveAttachment}
            thinkingSel={thinkingSel}
            thinkCap={chat.thinkCap}
            sending={chat.sending}
            searching={chat.searching}
            onSend={chat.onSend}
            onCancel={chat.onCancel}
            onRecall={chat.onRecall}
            onToggleWebSearch={chat.onToggleWebSearch}
            setError={chat.setError}
            onResumeFollow={resumeFollow}
            onOpenModelSwitch={() => setModelSwitchOpen(true)}
            onOpenAgentPicker={() => setAgentPickerOpen(true)}
            onSelectThinking={(sel) => setThinking(sessionId, sel)}
          />
          {/* 会话纲要:右缘贴边短横线(每条 = 一次回答),hover 展开完整浮层;两种模式都保留(P1) */}
          <OutlinePanel messages={messages} onJump={onOutlineJump} />
        </div>
        {/* 检查器(P3-1:两态通用;小窗紧凑态不渲染) */}
        {!compactTools && (
          <Inspector
            sessionId={sessionId}
            collapsed={!chatInspectorOpen}
            onToggleCollapse={toggleChatInspector}
          />
        )}
      </div>

      <ChatDialogs
        sessionId={sessionId}
        sessionAgentId={session?.agent_id ?? null}
        memoryBody={memoryDto?.body ?? ''}
        modelSwitchOpen={modelSwitchOpen}
        onCloseModelSwitch={() => setModelSwitchOpen(false)}
        onPickModel={onPickModel}
        retryTarget={chat.retryTarget}
        onCloseRetryModel={() => chat.setRetryTarget(null)}
        onPickRetryModel={chat.onPickRetryModel}
        agentPickerOpen={agentPickerOpen}
        onCloseAgentPicker={() => setAgentPickerOpen(false)}
        onSelectAgent={async (agentId) => {
          setAgentPickerOpen(false);
          try {
            await setSessionAgent(sessionId, agentId);
          } catch (e) {
            chat.setError(`切换智能体失败:${e instanceof Error ? e.message : String(e)}`);
          }
        }}
        handoff={handoff}
        handoffDefaultStrategy={handoffDefaultStrategy}
        onHandoffConfirm={onHandoffConfirm}
        onCancelHandoff={onCancelHandoff}
        showInjected={showInjected}
        switchBanner={switchBanner}
        onCloseInjected={() => setShowInjected(false)}
        toast={chat.toast}
        onCloseToast={chat.hideToast}
      />

      {/* 划词选中浮动工具栏 —— Portal 到 body,绝对定位在选区上方 */}
      {selTool && (
        <ChatSelectionToolbar
          selection={selTool}
          onClose={() => setSelTool(null)}
          onQuote={handleSelectionQuote}
        />
      )}
    </WorkbenchChromeContext.Provider>
  );
}
