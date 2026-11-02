import {
  GRAPH_HEIGHT,
  GRAPH_WIDTH,
  matchesEntity,
  nameMatches,
  normalizeQuery,
  truncateLabel,
  typeColor,
  type GraphLayout,
} from '../../lib/longTermMemory';

interface Props {
  layout: GraphLayout;
  /** 搜索词;命中节点高亮,未命中节点与两端都未命中的边淡出 */
  query: string;
}

/** 关系网络卡:零依赖 SVG 圆环布局。 */
export function RelationGraphCard({ layout, query }: Props) {
  const q = normalizeQuery(query);
  const showLabels = layout.nodes.length <= 48;

  return (
    <div className="set-card">
      <div className="set-card-h">
        关系网络
        <span className="st ltm-st">
          {layout.truncated
            ? `显示 ${layout.nodes.length}/${layout.total}`
            : `${layout.nodes.length} 节点`}
        </span>
      </div>
      <div className="set-card-b" style={{ padding: 0 }}>
        <svg
          className="ltm-graph"
          viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
          role="img"
          aria-label="长期记忆关系网络图"
        >
          {layout.edges.map((r, i) => {
            if (q && !nameMatches(r.from, q) && !nameMatches(r.to, q)) return null;
            const a = layout.nodeMap.get(r.from)!;
            const b = layout.nodeMap.get(r.to)!;
            return (
              <line
                key={`${r.from}-${r.to}-${r.relation_type}-${i}`}
                x1={a.x}
                y1={a.y}
                x2={b.x}
                y2={b.y}
                className="ltm-edge"
              />
            );
          })}
          {layout.nodes.map((n) => {
            const hit = !q || matchesEntity(n, q);
            return (
              <g key={n.name} opacity={hit ? 1 : 0.2}>
                <circle
                  cx={n.x}
                  cy={n.y}
                  r={n.r}
                  fill={typeColor(n.entity_type)}
                  fillOpacity={0.85}
                  stroke={q && hit ? 'var(--text-1)' : 'transparent'}
                  strokeWidth={2}
                />
                <title>{`${n.name}\n类型: ${n.entity_type}\n观测: ${n.observations.length} 条`}</title>
                {showLabels && (
                  <text x={n.x} y={n.y - n.r - 5} textAnchor="middle" className="ltm-node-label">
                    {truncateLabel(n.name, 12)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
