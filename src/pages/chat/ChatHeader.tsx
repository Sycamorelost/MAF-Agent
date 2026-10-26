import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  IconCheck,
  IconChevronDown,
  IconEdit,
  IconPullRequest,
  IconTrash,
  IconX,
  IconBrain,
} from '../../components/icons';
import { useMessagesStore } from '../../stores/messages';
import { useMemoryStore } from '../../stores/memory';
import { useProvidersStore } from '../../stores/providers';
import { useSessionsStore } from '../../stores/sessions';
import type { MemoryStrategy, SessionRow } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { estimateTokens } from '../../lib/memoryInject';

/** 分支显示名:用户自定义 > 默认(主分支 / 分支 xxxx)。 */
function branchLabel(b: ipc.BranchInfo | undefined, fallbackId: string): string {
  if (b?.name) return b.name;
  const id = b?.branch_id ?? fallbackId;
  return id === 'main' ? '主分支' : `分支 ${id.slice(0, 4)}`;
}

interface ChatHeaderProps {
  title: string;
  sessionId: string;
  activeBranch: string;
  /** 递增信号:同一分支内分叉后也强制重载分支列表 */
  branchRefresh: number;
  current: { providerId: string; model: string } | null;
  memCollapsed: boolean;
  outlineOpen: boolean;
  onToggleMemPanel: () => void;
  onToggleOutline: () => void;
  onOpenModelSwitch: () => void;
  onError: (msg: string) => void;
}

/** 会话头部:标题 / 分支指示条 / 本会话模型 / 记忆面板开关。 */
export function ChatHeader({
  title,
  sessionId,
  activeBranch,
  branchRefresh,
  current,
  memCollapsed,
  outlineOpen,
  onToggleMemPanel,
  onToggleOutline,
  onOpenModelSwitch,
  onError,
}: ChatHeaderProps) {
  // ---------- F4:会话分支树 ----------
  const setActiveBranch = useSessionsStore((s) => s.setActiveBranch);
  const [branches, setBranches] = useState<ipc.BranchInfo[]>([]);
  const [branchOpen, setBranchOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
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

  // F4:激活分支变化(切换/分叉/搜索跳转)时重载分支列表
  useEffect(() => {
    void loadBranches();
  }, [loadBranches, activeBranch, branchRefresh]);

  const onRenameBranch = async () => {
    if (!renaming) return;
    try {
      await ipc.renameBranch(sessionId, renaming.id, renaming.name);
    } catch (e) {
      onError(`重命名分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setRenaming(null);
    await loadBranches();
  };

  /** 删除分支(「确认放弃整支」:UI 两态确认,后端级联清理自有消息与 FTS)。 */
  const onDeleteBranch = async (bid: string) => {
    try {
      await ipc.deleteBranch(sessionId, bid);
    } catch (e) {
      onError(`删除分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
    setDeleting(null);
    // 若删除的是激活分支,后端已回退 main;刷新会话状态触发消息重载
    await useSessionsStore.getState().refresh();
    await useMessagesStore.getState().load(sessionId, true);
    await loadBranches();
  };

  /** 切换分支(store 更新 active_branch 后由页面 effect 统一重载消息)。 */
  const pickBranch = async (bid: string) => {
    setBranchOpen(false);
    if (bid === activeBranch) return;
    try {
      await setActiveBranch(sessionId, bid);
    } catch (e) {
      onError(`切换分支失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div className="chat-head">
      <span className="chat-title ellipsis">{title}</span>
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
                        <div className="branch-item-main" onClick={() => void pickBranch(b.branch_id)}>
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
        onClick={onOpenModelSwitch}
        title="切换本会话使用的模型"
      >
        <span className={`stat-dot ${current ? 'ok' : 'warn'}`} />
        {current ? current.model : '未配置模型'}
        {current && <span className="tag">{current.providerId}</span>}
        <IconChevronDown size={13} />
      </button>
      <button
        type="button"
        className={`chat-panel-toggle${outlineOpen ? ' on' : ''}`}
        onClick={onToggleOutline}
        title={outlineOpen ? '收起纲要面板' : '展开纲要面板'}
      >
        <span style={{ fontSize: 13, fontWeight: 700 }}>#</span>
      </button>
      <button
        type="button"
        className={`chat-panel-toggle${memCollapsed ? '' : ' on'}`}
        onClick={onToggleMemPanel}
        title={memCollapsed ? '展开记忆面板' : '收起记忆面板'}
      >
        <IconBrain size={15} />
      </button>
    </div>
  );
}

interface ModelHandoffParams {
  sessionId: string | undefined;
  session: SessionRow | undefined;
  current: { providerId: string; model: string } | null;
  buildSystemWith: (memBody: string, strategy: MemoryStrategy) => string;
}

/** D3.1/D3.2:本会话模型切换与记忆交接(弹窗 / 横幅由页面渲染,状态与流转集中在此)。 */
export function useModelHandoff({
  sessionId,
  session,
  current,
  buildSystemWith,
}: ModelHandoffParams) {
  const selectModel = useProvidersStore((s) => s.select);
  const setSessionModel = useSessionsStore((s) => s.setModel);
  const setMemoryStrategy = useSessionsStore((s) => s.setMemoryStrategy);
  const loadMemory = useMemoryStore((s) => s.load);
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

  // D8:交接弹窗预选策略(目标模型的单模型配置 > 会话当前策略)
  const modelConfig = useProvidersStore((s) => s.modelConfig);
  const handoffDefaultStrategy: MemoryStrategy = (() => {
    const fallback = session?.memory_injection_strategy ?? 'full';
    if (!handoff) return fallback;
    const i = handoff.to.indexOf('/');
    if (i < 0) return fallback;
    const pid = handoff.to.slice(0, i);
    const m = handoff.to.slice(i + 1);
    return modelConfig[pid]?.[m]?.memory_injection ?? fallback;
  })();

  return {
    handoff,
    switchBanner,
    setSwitchBanner,
    showInjected,
    setShowInjected,
    handoffDefaultStrategy,
    onPickModel,
    onCancelHandoff,
    onHandoffConfirm,
  };
}
