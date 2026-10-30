//! 工具流水页签(P6 / 设计稿):统计卡 + 筛选 chips(带计数)+ 时间线 + 失败重试/跳过
//! + 说明 + 底部状态行。数据来自 useWorkbenchData(与对话内 ToolTrace 共用 toolTrace.ts)。

import { useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { fmtMs, type SessionTrace } from '../../../../lib/toolTrace';
import { IconInfo, IconTerminal } from '../../../../components/icons';
import type { WorkbenchData } from '../../useWorkbenchData';

interface Item {
  s: SessionTrace;
  /** 稳定键:来源消息 id + 在原数组中的下标(「跳过」按此键隐藏)。 */
  k: string;
}

type Filter = 'all' | 'ok' | 'err';

function fmtTime(created: string): string {
  const m = /(\d{2}):(\d{2}):(\d{2})/.exec(created);
  return m ? `${m[1]}:${m[2]}:${m[3]}` : created;
}

const WRITE_TOOLS = new Set(['fs_write_text', 'fs_write', 'fs_export_docx', 'fs_export_xlsx', 'write_file']);

export function ToolStreamTab({ data }: { data: WorkbenchData }) {
  const [, setSearchParams] = useSearchParams();
  const [filter, setFilter] = useState<Filter>('all');
  /** 「跳过」的失败项(仅本视图内隐藏,不改动消息数据)。 */
  const [skipped, setSkipped] = useState<Set<string>>(new Set());

  const all: Item[] = useMemo(
    () => data.traces.map((s, i) => ({ s, k: `${s.messageId}:${i}` })),
    [data.traces],
  );

  const okCount = all.filter(({ s }) => s.trace.ok).length;
  const errCount = all.length - okCount;
  const writeCount = all.filter(({ s }) => WRITE_TOOLS.has(s.trace.tool)).length;
  const subCount = all.filter(({ s }) => s.trace.tool === 'delegate').length;
  const totalMs = all.reduce((acc, { s }) => acc + (s.trace.elapsedMs ?? 0), 0);
  const totalSec = (totalMs / 1000).toFixed(1);

  if (all.length === 0) {
    return <div className="insp-empty">还没有工具调用。模型执行文件/命令/检索等操作后会列在这里。</div>;
  }

  const visible = all.filter(
    ({ s, k }) =>
      !skipped.has(k) && (filter === 'all' || (filter === 'ok' ? s.trace.ok : !s.trace.ok)),
  );

  const filters: [Filter, string, number][] = [
    ['all', '全部', all.length],
    ['err', '失败', errCount],
    ['ok', '成功', okCount],
  ];

  return (
    <div className="insp-list insp-stream">
      {/* 统计卡 */}
      <div className="insp-sumcard">
        <div className="insp-sumcard-head">
          <span className="insp-sumcard-ico">
            <IconTerminal size={15} />
          </span>
          <span className="insp-sumcard-title">本次 {all.length} 次调用</span>
        </div>
        <div className="insp-sumcard-sub">
          成功 {okCount} · 失败 {errCount} · 写入 {writeCount} · 子代理 {subCount} · 合计{' '}
          {totalSec}s
        </div>
      </div>

      {/* 筛选 chips */}
      <div className="insp-stream-filters">
        {filters.map(([k, label, n]) => (
          <button
            key={k}
            type="button"
            className={`chip${filter === k ? ' on' : ''}`}
            onClick={() => setFilter(k)}
          >
            {label} {n}
          </button>
        ))}
      </div>

      {/* 时间线 */}
      <div className="ins-label">
        时间线
        <span className="sp" />
        <span className="n">倒序 · 保留 7 天</span>
      </div>
      {visible.length === 0 && <div className="insp-empty">该筛选下没有工具调用。</div>}
      {visible.map(({ s, k }) => {
        const jump = () => {
          if (s.messageId > 0) setSearchParams({ msg: String(s.messageId) });
        };
        return (
          <div key={k} className="insp-stream-item">
            <div className="insp-stream-line">
              <span className={`insp-stream-dot${s.trace.ok ? '' : ' err'}`} />
              <button
                type="button"
                className="insp-stream-main"
                disabled={s.messageId <= 0}
                title={s.messageId > 0 ? '跳到来源消息' : '正在生成'}
                onClick={jump}
              >
                <span className="insp-stream-tool">{s.trace.tool}</span>
                {s.trace.args && (
                  <span className="insp-stream-args" title={s.trace.args}>
                    {s.trace.args}
                  </span>
                )}
                <span className={`insp-stream-stat${s.trace.ok ? '' : ' err'}`}>
                  {s.trace.ok ? '成功' : '失败'}
                  {s.trace.elapsedMs != null && ` · ${fmtMs(s.trace.elapsedMs)}`}
                </span>
              </button>
              <span className="insp-stream-time">{fmtTime(s.created)}</span>
            </div>
            {!s.trace.ok && (
              <div className="insp-stream-ops">
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  disabled={s.messageId <= 0}
                  onClick={jump}
                >
                  查看输出
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  title="在本列表中跳过此项(不影响会话数据)"
                  onClick={() => setSkipped((prev) => new Set(prev).add(k))}
                >
                  跳过
                </button>
              </div>
            )}
          </div>
        );
      })}

      <div className="insp-note">
        <IconInfo size={12} />
        参数与输出已脱敏;点击任一调用可跳转来源消息。流水随会话保存,不写入 memory.md。
      </div>
      <div className="insp-statusbar">
        调用 {all.length} · 失败 {errCount}
        <span className="sp" />
        自动压缩 40 条
      </div>
    </div>
  );
}
