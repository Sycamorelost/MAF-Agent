//! 工作台检查器数据聚合(P6):四页签共享一份聚合,禁止各自重复解析消息。
//! 只读聚合:消息不复制副本,traces/artifacts 由 messages+streaming 派生(useMemo);
//! 备份与记忆为独立 IPC 数据,提供 refresh 供操作后重拉。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  fsBackupList,
  memoryBudgetStatus,
  readMemory,
  type BudgetStatus,
  type FsBackup,
  type MemoryDto,
  type MessageRow,
} from '../../lib/ipc';
import {
  artifactsFromTraces,
  collectSessionTraces,
  extractArtifacts,
  mergeArtifactLists,
  parseClosedTraces,
  type Artifact,
  type SessionTrace,
  type Trace,
} from '../../lib/toolTrace';
import { useMessagesStore } from '../../stores/messages';

const EMPTY_MESSAGES: MessageRow[] = [];

export interface WorkbenchData {
  messages: MessageRow[];
  streaming: string;
  traces: SessionTrace[];
  artifacts: Artifact[];
  backups: FsBackup[];
  budget: BudgetStatus | null;
  memory: MemoryDto | null;
  memoryError: string | null;
  refreshBackups: () => Promise<void>;
  refreshMemory: () => Promise<void>;
}

export function useWorkbenchData(sessionId: string): WorkbenchData {
  const messages = useMessagesStore((s) => s.bySession[sessionId] ?? EMPTY_MESSAGES);
  const streaming = useMessagesStore((s) => s.streaming[sessionId] ?? '');

  // 历史消息的工具痕迹:只在消息变化时解析一次
  const messageTraces = useMemo(() => collectSessionTraces(messages), [messages]);

  // P13:流式正文增量解析 —— 只处理上次之后新增的「已闭合」maf-tool 块,
  // 避免每 50ms 对不断变长的正文做全量重扫。
  const streamCacheRef = useRef<{ content: string; traces: Trace[] } | null>(null);
  const streamTraces = useMemo<Trace[]>(() => {
    if (!streaming) {
      streamCacheRef.current = null;
      return [];
    }
    const cache = streamCacheRef.current;
    if (cache && streaming.startsWith(cache.content)) {
      const { traces, consumed } = parseClosedTraces(streaming.slice(cache.content.length));
      cache.traces.push(...traces);
      cache.content = streaming.slice(0, cache.content.length + consumed);
      return [...cache.traces];
    }
    // 新流 / 内容被重置:从头解析一次
    const { traces, consumed } = parseClosedTraces(streaming);
    streamCacheRef.current = { content: streaming.slice(0, consumed), traces };
    return [...traces];
  }, [streaming]);

  // 工具流水:历史消息 + 流式增量(伪消息 id=-1,不可跳转)
  const traces = useMemo<SessionTrace[]>(
    () => [
      ...messageTraces,
      ...streamTraces.map((trace) => ({ messageId: -1, created: '', trace })),
    ],
    [messageTraces, streamTraces],
  );

  // 产物:历史消息解析一次;流式部分复用已解析 trace,不再按原文重扫
  const artifacts = useMemo<Artifact[]>(() => {
    const base = extractArtifacts(messages);
    if (streamTraces.length === 0) return base;
    const stepOffset = base.length > 0
      ? Math.max(...base.map((a) => a.step ?? 0)) + 1
      : 1;
    return mergeArtifactLists(base, artifactsFromTraces(streamTraces, stepOffset));
  }, [messages, streamTraces]);

  const [backups, setBackups] = useState<FsBackup[]>([]);
  const [budget, setBudget] = useState<BudgetStatus | null>(null);
  const [memory, setMemory] = useState<MemoryDto | null>(null);
  const [memoryError, setMemoryError] = useState<string | null>(null);

  const refreshBackups = useCallback(async () => {
    setBackups(await fsBackupList(sessionId));
  }, [sessionId]);

  const refreshMemory = useCallback(async () => {
    try {
      const [b, m] = await Promise.all([memoryBudgetStatus(sessionId), readMemory(sessionId)]);
      setBudget(b);
      setMemory(m);
      setMemoryError(null);
    } catch (e) {
      setMemoryError(e instanceof Error ? e.message : String(e));
    }
  }, [sessionId]);

  useEffect(() => {
    void refreshBackups().catch(() => {});
  }, [refreshBackups]);
  useEffect(() => {
    void refreshMemory();
  }, [refreshMemory]);

  // 一轮生成结束后自动重拉记忆/预算(否则检查器里的用量条会停留在旧值)
  const wasStreamingRef = useRef(false);
  // streaming 期间后端每次写文件自动备份入库,但前端只在结束时才拉;
  // 这里加一个 2s 节流的中途刷新,让「变更与回滚」面板实时可见新备份
  const backupsTimerRef = useRef<number | null>(null);
  useEffect(() => {
    const active = streaming.length > 0;
    if (wasStreamingRef.current && !active) {
      // 结束时刻:立即拉一次(并清除中途节流定时器,避免重复)
      if (backupsTimerRef.current != null) {
        window.clearTimeout(backupsTimerRef.current);
        backupsTimerRef.current = null;
      }
      void refreshMemory();
      void refreshBackups().catch(() => {});
    } else if (active) {
      // 中途:2s 节流拉一次
      if (backupsTimerRef.current != null) return;
      backupsTimerRef.current = window.setTimeout(() => {
        backupsTimerRef.current = null;
        void refreshBackups().catch(() => {});
      }, 2000);
    }
    wasStreamingRef.current = active;
  }, [streaming, refreshMemory, refreshBackups]);

  return {
    messages,
    streaming,
    traces,
    artifacts,
    backups,
    budget,
    memory,
    memoryError,
    refreshBackups,
    refreshMemory,
  };
}
