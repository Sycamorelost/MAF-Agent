/** 长期记忆(知识图谱)面板容器:负责数据加载 / 清除与子卡片编排。
 *  数据源:data/mcp-memory.jsonl(「长期记忆」MCP server 写入,跨会话共享)。
 *  纯计算见 lib/longTermMemory.ts,展示层见 components/longterm/*。
 *  同时用于设置页「记忆」分区与导航记忆页的「长期记忆」视图。 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { uiConfirm } from './confirmService';
import {
  mcpClearMemory,
  mcpMemoryGraph,
  type MemoryGraphInfo,
} from '../lib/ipc/mcp';
import { buildGraphLayout, buildTypeStats, fmtBytes } from '../lib/longTermMemory';
import { LongTermSummaryCard } from './longterm/SummaryCard';
import { TypeDistributionCard } from './longterm/TypeDistributionCard';
import { RelationGraphCard } from './longterm/RelationGraphCard';
import { EntityDetailsCard } from './longterm/EntityDetailsCard';

export function LongTermMemoryPanel() {
  const [data, setData] = useState<MemoryGraphInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [query, setQuery] = useState('');

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(await mcpMemoryGraph());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const entities = useMemo(() => data?.entities ?? [], [data]);
  const relations = useMemo(() => data?.relations ?? [], [data]);
  const typeStats = useMemo(() => buildTypeStats(entities), [entities]);
  const graph = useMemo(() => buildGraphLayout(entities, relations), [entities, relations]);

  const onClear = async () => {
    const ok = await uiConfirm({
      title: '清除长期记忆',
      message:
        '确定清除全部长期记忆数据?\n该数据由「长期记忆」MCP 工具跨会话共享,删除后不可恢复。',
      danger: true,
      confirmText: '清除',
    });
    if (!ok) return;
    try {
      const size = await mcpClearMemory();
      setMsg(size > 0 ? `已清除长期记忆数据(${fmtBytes(size)})` : '长期记忆数据为空,无需清除');
      await load();
    } catch (e) {
      setMsg(`清除失败: ${e}`);
    }
  };

  const empty = !loading && !error && entities.length === 0;

  return (
    <>
      <LongTermSummaryCard
        data={data}
        loading={loading}
        error={error}
        msg={msg}
        onRefresh={() => void load()}
        onClear={() => void onClear()}
      />

      {empty && (
        <div className="set-card">
          <div className="set-card-b">
            <div className="set-hint">
              还没有长期记忆数据。在对话中让「长期记忆」MCP 工具记录实体与关系后,这里会显示知识图谱。
            </div>
          </div>
        </div>
      )}

      {!empty && !error && (
        <>
          <div className="ltm-charts">
            <TypeDistributionCard stats={typeStats} total={entities.length} />
            <RelationGraphCard layout={graph} query={query} />
          </div>
          <EntityDetailsCard
            entities={entities}
            relations={relations}
            query={query}
            onQueryChange={setQuery}
          />
        </>
      )}
    </>
  );
}
