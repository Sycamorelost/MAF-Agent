//! 用量统计 · 分类统计(从 UsageView 原样拆出,零行为变更)。

import * as ipc from '../../../lib/ipc';
import type { UsageData, UsageDim } from '../useUsageData';
import { fmtCostNano, fmtNum } from './format';

const DIM_OPTIONS: { value: UsageDim; label: string }[] = [
  { value: 'model', label: '按模型' },
  { value: 'provider', label: '按服务商' },
  { value: 'key', label: '按 Key' },
  { value: 'agent', label: '按助手' },
];

export function UsageGroups({ data }: { data: UsageData }) {
  const groups = data.summary?.groups ?? [];
  const maxReq = groups.reduce((m, g) => (g.requests > m ? g.requests : m), 0) || 1;

  return (
    <div className="set-card">
      <div className="set-card-h">
        分类统计
        <span className="sp" />
        <div className="seg">
          {DIM_OPTIONS.map((o) => (
            <button
              key={o.value}
              className={data.dim === o.value ? 'on' : ''}
              onClick={() => data.setDim(o.value)}
            >
              {o.label}
            </button>
          ))}
        </div>
      </div>
      <div className="set-card-b">
        {groups.length === 0 ? (
          <div className="set-hint">该时间范围内暂无用量数据</div>
        ) : (
          groups.map((g) => (
            <div
              className={`usage-bar-row${data.drill === g.key ? ' on' : ''}`}
              key={g.key}
              onClick={() => data.setDrill(data.drill === g.key ? null : g.key)}
              role="button"
              title="点击下钻:只看该分类的请求明细"
            >
              <span className="nm" title={g.name}>
                {g.name}
                {g.cooling && <span className="cool-num">冷却中</span>}
              </span>
              <span className="bar">
                <i style={{ width: `${Math.max(3, Math.round((g.requests / maxReq) * 100))}%` }} />
              </span>
              <span className="num">
                {fmtNum(g.requests)} 次 · {ipc.formatTokens(g.tokens)}
                {g.cost_nano > 0 ? ` · ${fmtCostNano(g.cost_nano)}` : ''}
                {g.uncosted_requests > 0 ? ` · 未计价 ${g.uncosted_requests}` : ''}
                {g.failed > 0 ? ` · 失败 ${g.failed}` : ''}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
