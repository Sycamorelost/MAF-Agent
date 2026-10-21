//! 用量统计 · 总览卡片(从 UsageView 原样拆出,零行为变更)。

import * as ipc from '../../../lib/ipc';
import type { UsageData } from '../useUsageData';
import { fmtCostNano, fmtNum, pct } from './format';

export function UsageOverview({ data }: { data: UsageData }) {
  const totals = data.summary?.totals;
  const failRate =
    totals && totals.requests > 0 ? `${((totals.failed / totals.requests) * 100).toFixed(1)}%` : '0%';
  const avgFirst = totals?.avg_first_token_ms != null ? `${totals.avg_first_token_ms} ms` : '—';

  return (
    <div className="set-card">
      <div className="set-card-h">
        总览
        <span className="sp" />
        <div className="seg">
          {[7, 30, 90].map((d) => (
            <button key={d} className={data.days === d ? 'on' : ''} onClick={() => data.setDays(d)}>
              近 {d} 天
            </button>
          ))}
        </div>
      </div>
      <div className="set-card-b">
        <div className="usage-metrics">
          <div className="usage-metric">
            <div className="v">{fmtNum(totals?.requests ?? 0)}</div>
            <div className="k">总请求数</div>
          </div>
          <div className="usage-metric">
            <div className="v">{ipc.formatTokens(totals?.tokens ?? 0)}</div>
            <div className="k">总 Token 用量</div>
          </div>
          <div className="usage-metric">
            <div className="v">{failRate}</div>
            <div className="k">失败率(限流 {fmtNum(totals?.rate_limited ?? 0)} 次)</div>
          </div>
          <div className="usage-metric">
            <div className="v">{avgFirst}</div>
            <div className="k">平均首字延迟</div>
          </div>
          <div className="usage-metric">
            <div className="v">{fmtCostNano(totals?.cost_nano)}</div>
            <div className="k">
              总成本(按定价表估算)
              {totals && totals.successful_requests > 0 && (
                <>
                  {' '}
                  · 覆盖 {pct(totals.cost_coverage)}
                  {totals.uncosted_requests > 0 ? ` · 未计价 ${fmtNum(totals.uncosted_requests)} 次` : ''}
                </>
              )}
            </div>
          </div>
        </div>
        {totals && totals.uncosted_requests > 0 && (
          <div className="set-hint" style={{ marginTop: 8 }}>
            未计价请求:厂商未返回 input/output 拆分、或该模型无定价卡,成本不计入总额,避免显示成 ¥0 的假账。
          </div>
        )}
      </div>
    </div>
  );
}
