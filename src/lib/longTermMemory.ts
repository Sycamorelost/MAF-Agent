//! 长期记忆(知识图谱)的纯计算逻辑:统计、布局、搜索索引。
//! 与 UI 组件解耦,便于单测;数据源见 lib/ipc/mcp.ts 的 MemoryGraphInfo。

import type { MemoryEntityInfo, MemoryRelationInfo } from './ipc/mcp';

/** 类型配色池;按类型名哈希取色,同一类型始终同一颜色。 */
const TYPE_COLORS = [
  '#4f8cff',
  '#22b07d',
  '#e0a63a',
  '#b56bd6',
  '#e06b6b',
  '#3fb6c8',
  '#8a8f98',
  '#c07b4a',
];

/** SVG 关系图画布尺寸(viewBox)。 */
export const GRAPH_WIDTH = 640;
export const GRAPH_HEIGHT = 380;

/** 关系图最多渲染的实体数(按类型聚集、观测数降序取前 N,避免大图谱卡顿)。 */
export const MAX_GRAPH_NODES = 160;

export interface TypeStat {
  type: string;
  count: number;
  observations: number;
}

export interface GraphNode extends MemoryEntityInfo {
  x: number;
  y: number;
  r: number;
}

export interface GraphLayout {
  nodes: GraphNode[];
  edges: MemoryRelationInfo[];
  nodeMap: Map<string, GraphNode>;
  total: number;
  truncated: boolean;
}

/** 类型 → 稳定配色(按名称哈希,同一类型颜色恒定)。 */
export function typeColor(type: string): string {
  let h = 0;
  for (let i = 0; i < type.length; i++) h = (h * 31 + type.charCodeAt(i)) >>> 0;
  return TYPE_COLORS[h % TYPE_COLORS.length];
}

export function fmtBytes(n: number): string {
  if (n <= 0) return '0 B';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function truncateLabel(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}

/** 搜索词归一化(去空白 + 小写);空串表示不过滤。 */
export function normalizeQuery(query: string): string {
  return query.trim().toLowerCase();
}

/** 类型统计(按实体数量降序,数量相同按类型名排序)。 */
export function buildTypeStats(entities: MemoryEntityInfo[]): TypeStat[] {
  const map = new Map<string, { count: number; observations: number }>();
  for (const e of entities) {
    const cur = map.get(e.entity_type) ?? { count: 0, observations: 0 };
    cur.count += 1;
    cur.observations += e.observations.length;
    map.set(e.entity_type, cur);
  }
  return [...map.entries()]
    .map(([type, v]) => ({ type, ...v }))
    .sort((a, b) => b.count - a.count || a.type.localeCompare(b.type));
}

/**
 * 关系图布局:类型聚集 + 观测数降序,等角圆环排布;
 * 超出 MAX_GRAPH_NODES 时只保留前 N 个,并过滤两端节点都不在图中 的边。
 */
export function buildGraphLayout(
  entities: MemoryEntityInfo[],
  relations: MemoryRelationInfo[],
): GraphLayout {
  const byType = buildTypeStats(entities);
  const ordered: MemoryEntityInfo[] = [];
  for (const t of byType) {
    ordered.push(
      ...entities
        .filter((e) => e.entity_type === t.type)
        .sort((a, b) => b.observations.length - a.observations.length),
    );
    if (ordered.length >= MAX_GRAPH_NODES) break;
  }

  const picked = ordered.slice(0, MAX_GRAPH_NODES);
  const n = picked.length;
  const cx = GRAPH_WIDTH / 2;
  const cy = GRAPH_HEIGHT / 2;
  const radius = n <= 1 ? 0 : Math.min(150, 60 + n * 3);
  const nodes: GraphNode[] = picked.map((e, i) => {
    const angle = (i / Math.max(1, n)) * Math.PI * 2 - Math.PI / 2;
    return {
      ...e,
      x: cx + radius * Math.cos(angle),
      y: cy + radius * Math.sin(angle),
      r: 5 + Math.min(9, e.observations.length),
    };
  });

  const nodeMap = new Map(nodes.map((node) => [node.name, node]));
  const edges = relations.filter((r) => nodeMap.has(r.from) && nodeMap.has(r.to));

  return { nodes, edges, nodeMap, total: entities.length, truncated: entities.length > picked.length };
}

/** 实体 → 关联关系描述(入边标 ←,出边标 →),用于明细列表。 */
export function buildRelationsOf(relations: MemoryRelationInfo[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const r of relations) {
    map.set(r.from, [...(map.get(r.from) ?? []), `→ ${r.to}(${r.relation_type})`]);
    map.set(r.to, [...(map.get(r.to) ?? []), `← ${r.from}(${r.relation_type})`]);
  }
  return map;
}

/** 实体是否命中搜索词(名称 / 类型 / 任一条观测)。q 需已归一化。 */
export function matchesEntity(e: MemoryEntityInfo, q: string): boolean {
  return (
    !q ||
    e.name.toLowerCase().includes(q) ||
    e.entity_type.toLowerCase().includes(q) ||
    e.observations.some((o) => o.toLowerCase().includes(q))
  );
}

/** 仅按名称匹配(关系图里用于判断边是否命中搜索)。q 需已归一化。 */
export function nameMatches(name: string, q: string): boolean {
  return !q || name.toLowerCase().includes(q);
}
