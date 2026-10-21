//! 用量统计 · 最近请求明细(从 UsageView 原样拆出,零行为变更)。

import { Fragment, useState } from 'react';
import type { UsageData } from '../useUsageData';
import { fmtClock, fmtCostNano, fmtNum, snapshotLines } from './format';

const STATUS_LABEL: Record<string, string> = {
  success: '成功',
  failed: '失败',
  rate_limited: '限流',
};

const KIND_LABEL: Record<string, string> = {
  stream: '流中断',
  auth: '鉴权失败',
  server: '服务端错误',
  network: '网络错误',
  bad_request: '请求错误',
  rate_limited: '限流',
  not_supported: '不支持',
  parse: '解析失败',
  other: '其他',
};

export function UsageEntries({ data }: { data: UsageData }) {
  const [openRow, setOpenRow] = useState<number | null>(null);
  const groups = data.summary?.groups ?? [];
  const drillLabel = data.drill ? groups.find((g) => g.key === data.drill)?.name ?? data.drill : null;

  return (
    <div className="set-card">
      <div className="set-card-h">
        最近请求明细
        {drillLabel && (
          <>
            <span className="sp" />
            <span className="usage-drill">
              已下钻:{drillLabel}
              <button onClick={() => data.setDrill(null)}>清除</button>
            </span>
          </>
        )}
      </div>
      <div className="set-card-b" style={{ padding: 0 }}>
        <div className="usage-scroll">
          <table className="usage-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>模型</th>
                <th>助手</th>
                <th>槽位</th>
                <th>Key</th>
                <th>状态</th>
                <th>Token</th>
                <th>缓存命中</th>
                <th>成本</th>
                <th>首字</th>
              </tr>
            </thead>
            <tbody>
              {data.entries.map((e) => (
                <Fragment key={e.id}>
                  <tr
                    onClick={() => setOpenRow((id) => (id === e.id ? null : e.id))}
                    title="点击查看当次计价快照"
                    style={{ cursor: 'pointer' }}
                  >
                    <td className="mono">{fmtClock(e.created_at)}</td>
                    <td>{e.model_id}</td>
                    <td>{e.agent_name ?? '—'}</td>
                    <td className="mono">
                      #{e.slot_id}
                      {e.slot_label ? ` ${e.slot_label}` : ''}
                    </td>
                    <td className="mono">{e.key_masked ?? '—'}</td>
                    <td
                      className={
                        e.status === 'success'
                          ? 'st-ok'
                          : e.status === 'rate_limited'
                            ? 'st-rl'
                            : 'st-fail'
                      }
                    >
                      {STATUS_LABEL[e.status] ?? e.status}
                      {e.error_kind ? ` · ${KIND_LABEL[e.error_kind] ?? e.error_kind}` : ''}
                    </td>
                    <td className="mono">{e.total_tokens != null ? fmtNum(e.total_tokens) : '—'}</td>
                    <td className="mono">
                      {e.cached_tokens != null ? fmtNum(e.cached_tokens) : '未知'}
                    </td>
                    <td className="mono">
                      {fmtCostNano(e.cost_nano)}
                      {e.cost_source === 'unpriced' && <span className="set-hint"> 未计价</span>}
                    </td>
                    <td className="mono">{e.first_token_ms != null ? `${e.first_token_ms}ms` : '—'}</td>
                  </tr>
                  {openRow === e.id && (
                    <tr>
                      <td colSpan={10}>
                        <div className="set-hint">
                          {e.cost_source ? `来源:${e.cost_source} · ` : ''}
                          {e.pricing_snapshot ? (
                            snapshotLines(e.pricing_snapshot).map((line) => <div key={line}>{line}</div>)
                          ) : (
                            <div>无计价快照(厂商未提供价格或未匹配到模型卡)。</div>
                          )}
                        </div>
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
              {data.entries.length === 0 && (
                <tr>
                  <td colSpan={10}>暂无记录</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
