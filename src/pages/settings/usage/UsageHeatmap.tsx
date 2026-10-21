//! 用量统计 · 时段热力图(从 UsageView 原样拆出,零行为变更)。

import { Fragment } from 'react';
import * as ipc from '../../../lib/ipc';
import type { UsageData } from '../useUsageData';
import { fmtNum } from './format';

const DOW_LABEL = ['日', '一', '二', '三', '四', '五', '六'];

export function UsageHeatmap({ data }: { data: UsageData }) {
  const heatCells = new Map<string, ipc.UsageHour>();
  for (const h of data.hours) heatCells.set(`${h.dow}-${h.hour}`, h);
  const heatMax = data.hours.reduce((m, h) => (h.requests > m ? h.requests : m), 0) || 1;

  return (
    <div className="set-card">
      <div className="set-card-h">
        时段热力图
        <span className="sp" />
        <span className="set-hint">星期 × 小时,颜色越深请求越多</span>
      </div>
      <div className="set-card-b">
        {data.hours.length === 0 ? (
          <div className="set-hint">该时间范围内暂无用量数据</div>
        ) : (
          <div className="usage-scroll">
            <div className="usage-heat" style={{ minWidth: 520 }}>
              <div className="usage-heat-corner" />
              {Array.from({ length: 24 }, (_, h) => (
                <div className="usage-heat-hd" key={`h${h}`}>
                  {h % 3 === 0 ? h : ''}
                </div>
              ))}
              {DOW_LABEL.map((dl, dow) => (
                <Fragment key={`d${dow}`}>
                  <div className="usage-heat-dow">周{dl}</div>
                  {Array.from({ length: 24 }, (_, h) => {
                    const cell = heatCells.get(`${dow}-${h}`);
                    const v = cell?.requests ?? 0;
                    const alpha = v === 0 ? 0 : 0.15 + 0.85 * (v / heatMax);
                    return (
                      <div
                        key={`c${dow}-${h}`}
                        className="usage-heat-cell"
                        title={`周${dl} ${h}:00 · ${fmtNum(v)} 次 · ${ipc.formatTokens(cell?.tokens ?? 0)} tokens`}
                        style={v === 0 ? undefined : { background: `rgba(59,130,246,${alpha.toFixed(2)})` }}
                      />
                    );
                  })}
                </Fragment>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
