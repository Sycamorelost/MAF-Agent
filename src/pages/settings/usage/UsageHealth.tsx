//! 用量统计 · Key 健康与成本复算(从 UsageView 原样拆出,零行为变更)。

import type { UsageData } from '../useUsageData';
import { fmtCostNano, fmtDelta, fmtNum } from './format';

export function UsageHealth({ data }: { data: UsageData }) {
  const healthRows = Object.entries(data.health).flatMap(([pid, rows]) =>
    rows.map((g) => ({ ...g, provider_id: pid })),
  );
  const recalc = data.recalc;

  return (
    <>
      {healthRows.length > 0 && (
        <div className="set-card">
          <div className="set-card-h">Key 健康</div>
          <div className="set-card-b" style={{ padding: 0 }}>
            <div className="usage-scroll">
              <table className="usage-table">
                <thead>
                  <tr>
                    <th>服务商</th>
                    <th>槽位</th>
                    <th>请求</th>
                    <th>成功</th>
                    <th>失败</th>
                    <th>限流</th>
                    <th>成功率</th>
                    <th>首字</th>
                  </tr>
                </thead>
                <tbody>
                  {healthRows.map((r) => {
                    const ok = r.requests - r.failed;
                    const rate = r.requests > 0 ? `${((ok / r.requests) * 100).toFixed(1)}%` : '—';
                    return (
                      <tr key={`${r.provider_id}-${r.key}`}>
                        <td>{r.provider_id}</td>
                        <td className="mono">
                          #{r.slot_id ?? '—'}
                          {r.slot_id != null ? ` ${r.name}` : ''}
                        </td>
                        <td className="mono">{fmtNum(r.requests)}</td>
                        <td className="mono st-ok">{fmtNum(ok)}</td>
                        <td className={`mono${r.failed > 0 ? ' st-fail' : ''}`}>{fmtNum(r.failed)}</td>
                        <td className={`mono${r.rate_limited > 0 ? ' st-rl' : ''}`}>
                          {fmtNum(r.rate_limited)}
                        </td>
                        <td className="mono">{rate}</td>
                        <td className="mono">
                          {r.avg_first_token_ms != null ? `${r.avg_first_token_ms}ms` : '—'}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      <div className="set-card">
        <div className="set-card-h">
          成本复算(只读)
          <span className="sp" />
          <button className="btn-ghost" disabled={data.recalcBusy} onClick={data.runRecalc}>
            {data.recalcBusy ? '试算中…' : '按当前定价试算'}
          </button>
        </div>
        <div className="set-card-b" style={{ padding: 0 }}>
          {!recalc ? (
            <div className="set-hint" style={{ padding: 12 }}>
              调价或修正汇率后,可按当前定价表试算近 {data.days} 天的历史成本,只报告差异,不修改落库数据。
            </div>
          ) : (
            <div className="usage-scroll">
              <table className="usage-table">
                <thead>
                  <tr>
                    <th>服务商 / 模型</th>
                    <th>请求</th>
                    <th>落库成本</th>
                    <th>当前定价试算</th>
                    <th>差异</th>
                  </tr>
                </thead>
                <tbody>
                  {recalc.rows.map((r) => (
                    <tr key={`${r.provider_id}/${r.model_id}`}>
                      <td>
                        <span className="mono">{r.provider_id}</span> / {r.model_id}
                        {r.unpriced_requests > 0 && (
                          <span className="set-hint"> · 当时未计价 {r.unpriced_requests}</span>
                        )}
                      </td>
                      <td className="mono">{fmtNum(r.requests)}</td>
                      <td className="mono">{fmtCostNano(r.stored_nano)}</td>
                      <td className="mono">{fmtCostNano(r.current_nano)}</td>
                      <td className={`mono${r.delta_nano && r.delta_nano > 0 ? ' st-fail' : ' st-ok'}`}>
                        {fmtDelta(r.delta_nano)}
                      </td>
                    </tr>
                  ))}
                  <tr>
                    <td>
                      <strong>合计</strong>
                    </td>
                    <td />
                    <td className="mono">{fmtCostNano(recalc.stored_total_nano)}</td>
                    <td className="mono">{fmtCostNano(recalc.current_total_nano)}</td>
                    <td className="mono">{fmtDelta(recalc.current_total_nano - recalc.stored_total_nano)}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
