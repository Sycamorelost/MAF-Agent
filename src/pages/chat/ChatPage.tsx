import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import type { VirtualizerHandle } from 'virtua';
import { mcpClearSessionAllowed } from '../../lib/ipc/mcp';
import StreamingBubble from '../../components/StreamingBubble';
import Composer from '../../components/Composer';
import Toast from '../../components/Toast';
import { ResizeHandle } from '../../components/ResizeHandle';
import { usePanelWidth } from '../../lib/usePanelWidth';
import MemPanel from '../../components/MemPanel';
import MemoryHandoffDialog from '../../components/MemoryHandoffDialog';
import ModelSwitchDialog from '../../components/ModelSwitchDialog';
import AgentPickerDialog from '../../components/AgentPickerDialog';
import OutlinePanel from '../../components/OutlinePanel';
import { agentIconComponent } from '../../components/icons';
import { Inspector } from '../../components/inspector/Inspector';
import { WorkbenchChromeContext } from '../../lib/workbenchChrome';

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
import { useChatInspector } from './useChatInspector';
import { ChatBanners } from './ChatBanners';
import { ChatEmptyState } from './ChatEmptyState';
import { buildQuote, type QuoteRef } from '../../lib/quote';

/** 稳定空数组:避免 `?? []` 每次渲染生成新引用导致 MessageList memo 失效。 */
const EMPTY_MESSAGES: MessageRow[] = [];

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
  const scrollRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<VirtualizerHandle | null>(null);
  const listWrapRef = useRef<HTMLDivElement>(null);
  /** P3:虚拟列表在滚动容器内的起始偏移(上方横幅/任务清单等兄弟节点占位) */
  const [listMargin, setListMargin] = useState(0);

  const session = sessions.find((s) => s.id === sessionId);
  const messages = sessionId ? (bySession[sessionId] ?? EMPTY_MESSAGES) : EMPTY_MESSAGES;

  // ---------- D16 / val.md §5.8:工作台能力永久开启(工作台页已并入会话页) ----------
  // isWork 已删除,PresetChip / Inspector / MemPanel 永远按工作台能力呈现

  // ---------- 检查器(P3-1:工作台能力永久开启,检查器默认开;与记忆面板互斥,右栏同时只开一个) ----------
  const { chatInspectorOpen, toggleChatInspector, onToggleMemPanel, chromeValue } =
    useChatInspector(memPanel.collapsed, memPanel.toggleCollapse);

  /** 前插历史标记:加载更早一页时抑制「自动滚到底部」,避免把视口甩走 */
  const prependingRef = useRef(false);

  /** 加载更早一页并保持视口位置(Virtualizer shift 以列表尾部为锚点自动补偿高度差) */
  const onLoadOlder = async () => {
    if (!sessionId) return;
    if (!useMessagesStore.getState().hasMore[sessionId]) return;
    prependingRef.current = true;
    await loadOlder(sessionId);
  };

  // ---------- P3:虚拟列表起始偏移测量(横幅/任务清单等上方兄弟节点高度会动态变化) ----------
  useLayoutEffect(() => {
    const measure = () => {
      const wrap = listWrapRef.current;
      const sc = scrollRef.current;
      if (!wrap || !sc) return;
      const m = Math.max(
        0,
        Math.round(wrap.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop),
      );
      setListMargin((prev) => (Math.abs(prev - m) <= 1 ? prev : m));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    const sc = scrollRef.current;
    if (sc) {
      ro.observe(sc);
      if (sc.firstElementChild) ro.observe(sc.firstElementChild);
    }
    return () => ro.disconnect();
  }, [sessionId]);
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

  // 全局搜索跳转定位
  const [searchParams, setSearchParams] = useSearchParams();
  const locatingRef = useRef(false);
  useEffect(() => {
    locatingRef.current = false;
  }, [sessionId, searchParams]);
  useEffect(() => {
    const target = searchParams.get('msg');
    if (!target || messages.length === 0) return;
    const idx = messages.findIndex((m) => String(m.id) === target);
    if (idx >= 0) {
      // P3:虚拟列表只挂载视口附近消息,离屏目标用 index 定位;
      // 定位后等待条目挂载再闪烁高亮(高度测量可能需几帧)
      listRef.current?.scrollToIndex(idx, { align: 'center' });
      let attempts = 0;
      let raf = 0;
      let timer = 0;
      const highlight = () => {
        const el = document.getElementById(`msg-${target}`);
        if (!el) {
          if (++attempts < 12) {
            raf = requestAnimationFrame(highlight);
          } else {
            setSearchParams({}, { replace: true });
          }
          return;
        }
        el.scrollIntoView({ block: 'center' });
        el.classList.add('msg-flash');
        timer = window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
        setSearchParams({}, { replace: true });
      };
      raf = requestAnimationFrame(highlight);
      return () => {
        cancelAnimationFrame(raf);
        if (timer) window.clearTimeout(timer);
      };
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
  }, [searchParams, messages, setSearchParams, hasMore, sessionId, loadOlder]);

  /** 纲要跳转:定位到消息首行;虚拟列表只挂载视口附近消息,先按 index 滚动,待条目挂载后再 flash。 */
  const onOutlineJump = useCallback(
    (msgId: number) => {
      const idx = messages.findIndex((m) => m.id === msgId);
      if (idx < 0) return;
      listRef.current?.scrollToIndex(idx, { align: 'start' });
      let attempts = 0;
      const highlight = () => {
        const el = document.getElementById(`msg-${msgId}`);
        if (!el) {
          if (++attempts < 12) requestAnimationFrame(highlight);
          return;
        }
        el.scrollIntoView({ block: 'start' });
        el.classList.add('msg-flash');
        window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
      };
      requestAnimationFrame(highlight);
    },
    [messages],
  );

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

  // 新消息到达 / 切换会话时:平滑滚动到底部
  // (?msg 定位期间不抢滚动,否则会把 scrollIntoView 的目标甩到底部;
  //  用 ref 读取最新目标,避免定位完成后清空 query 触发本 effect 再次拉到底部)
  const msgTargetRef = useRef<string | null>(null);
  msgTargetRef.current = searchParams.get('msg');
  /** 用户是否停在底部(跟随新内容);一旦手动上滚即暂停,滚回底部恢复。 */
  const followRef = useRef(true);
  /** 换会话时重置为跟随 */
  useEffect(() => {
    followRef.current = true;
  }, [sessionId]);
  useEffect(() => {
    if (prependingRef.current) {
      prependingRef.current = false;
      return;
    }
    if (msgTargetRef.current) return;
    // 用户正在上滚查看时不抢滚动,避免「一有输出就被拽回底部」
    if (!followRef.current) return;
    // P3:虚拟列表条目按需挂载,直接 scrollTo(scrollHeight) 会因高度估算偏短;
    // 先按 index 定位到最后一条(库内部会在测量后校正),再补滚动一次以覆盖列表下方元素。
    const n = messages.length;
    if (n > 0) listRef.current?.scrollToIndex(n - 1, { align: 'end' });
    const sc = scrollRef.current;
    const raf = requestAnimationFrame(() => {
      sc?.scrollTo({ top: sc.scrollHeight, behavior: 'smooth' });
    });
    return () => cancelAnimationFrame(raf);
  }, [sessionId, messages.length]);

  /** 距底部 80px 内视为「跟随」,否则用户已主动上滚查看。 */
  const onScrollFollow = () => {
    const sc = scrollRef.current;
    if (!sc) return;
    followRef.current = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  };

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
    <WorkbenchChromeContext.Provider value={chromeValue}>
      {/* 作为 .wb-main / .main 的 flex 项:必须 minWidth:0,否则长代码行等内容的最小内容宽度会撑破中栏 */}
      <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
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
          <ChatHeader
            title={session?.title ?? '对话'}
            sessionId={sessionId}
            activeBranch={activeBranch}
            branchRefresh={branchRefresh}
            memCollapsed={memPanel.collapsed}
            onToggleMemPanel={onToggleMemPanel}
            onError={chat.setError}
          />
          <div className="chat-scroll" ref={scrollRef} onScroll={onScrollFollow}>
            <div className="chat-inner">
              <ChatBanners
                switchBanner={switchBanner}
                compressNotice={chat.compressNotice}
                onShowInjected={() => setShowInjected(true)}
                onCloseSwitch={() => setSwitchBanner(null)}
                onCloseCompress={() => chat.setCompressNotice(null)}
              />
              {/* 对话任务清单:工作台模式也仍显示在消息区 */}
              <ChatTodos
                sessionId={sessionId}
                onNotice={chat.setCompressNotice}
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
                  scrollRef={scrollRef}
                  listRef={listRef}
                  startMargin={listMargin}
                  lastAssistantId={lastAssistantId}
                  onQuote={handleQuote}
                  onTranslate={chat.onTranslate}
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
          <Composer
            value={composerText}
            onChange={setComposerText}
            onSend={() => {
              // 用户主动发送:恢复「跟随到底部」
              followRef.current = true;
              chat.onSend();
            }}
            disabled={chat.sending || chat.searching}
            variant="docked"
            sending={chat.sending}
            onStop={chat.onCancel}
            attachments={pendingAttachments}
            onPickFiles={handlePickFiles}
            onRemoveAttachment={handleRemoveAttachment}
            quotes={quotes}
            onRemoveQuote={(qid) => setQuotes((prev) => prev.filter((q) => q.id !== qid))}
            onRecall={chat.onRecall}
            webSearch={session?.web_search_enabled ?? false}
            onToggleWebSearch={chat.onToggleWebSearch}
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
            model={{
              name: current ? current.model : '未配置模型',
              ready: !!current,
              onOpen: () => setModelSwitchOpen(true),
            }}
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
          {/* 会话纲要:右缘贴边短横线(每条 = 一次回答),hover 展开完整浮层;两种模式都保留(P1) */}
          <OutlinePanel messages={messages} onJump={onOutlineJump} />
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
        {/* 检查器(P3-1:两态通用,与记忆面板互斥) */}
        <Inspector
          sessionId={sessionId}
          collapsed={!chatInspectorOpen}
          onToggleCollapse={toggleChatInspector}
        />
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

      {chat.toast && <Toast text={chat.toast} onClose={chat.hideToast} />}
    </WorkbenchChromeContext.Provider>
  );
}
