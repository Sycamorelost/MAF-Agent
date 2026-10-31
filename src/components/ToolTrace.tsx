//! 工具调用痕迹 —— 正文里一行说明「做了什么」,点开才展开完整输出(对标截图 3/4)。
//! 数据由 useChatSend 注入为 ```maf-tool 围栏块(单行 JSON),持久化后仍按此渲染。
//! 折叠行 24px 高;操作全部幽灵(重试 / 查看完整输出 / 跳过此步);状态色仅 ok / err。

import { useState } from 'react';
import {
  IconChevronDown,
  IconChevronRight,
  IconFileText,
  IconLayers,
  IconRefresh,
  IconSearch,
  IconSkipForward,
  IconTerminal,
} from './icons';
import { SubagentCard } from './SubagentCard';
import { actionLabel, argSummary, fmtMs, parseTrace } from '../lib/toolTrace';

/** 工具名 → 线性图标(未知工具用分层图标兜底)。 */
function toolIcon(tool: string) {
  if (/(shell|command|exec|terminal)/.test(tool)) return IconTerminal;
  if (/(search|query|find|read|list|tree|info)/.test(tool)) {
    return /read|list|tree|info/.test(tool) ? IconFileText : IconSearch;
  }
  if (/(file|path|doc|write|export|zip)/.test(tool)) return IconFileText;
  return IconLayers;
}

/** 连续工具调用组(≥2 条):默认折叠为一行汇总,展开后逐条渲染。 */
export function ToolTraceGroup({ raws }: { raws: string[] }) {
  const [open, setOpen] = useState(false);
  const traces = raws.map(parseTrace);
  const total = raws.length;
  const errs = traces.filter((t) => t && !t.ok).length;
  const elapsedSum = traces.reduce((acc, t) => acc + (t?.elapsedMs ?? 0), 0);
  const elapsed = elapsedSum > 0 ? fmtMs(elapsedSum) : '';

  return (
    <div className={`tool-group${open ? ' open' : ''}${errs > 0 ? ' err' : ''}`}>
      <button
        type="button"
        className={`tool-row tool-group-row${open ? ' open' : ''}`}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        title={open ? '收起工具调用' : '展开工具调用'}
      >
        {open ? <IconChevronDown size={12} /> : <IconChevronRight size={12} />}
        <IconLayers size={12} />
        <span className="tname">工具调用 ×{total}</span>
        <span className="tsp" />
        {elapsed && <span className="ttime">{elapsed}</span>}
        {errs > 0 && <span className="tstat err">{errs} 失败</span>}
        <span className="tstat ok">{total - errs} 成功</span>
      </button>
      {open && (
        <div className="tool-group-body">
          {raws.map((raw, i) => (
            <ToolTrace key={i} raw={raw} />
          ))}
        </div>
      )}
    </div>
  );
}

export function ToolTrace({ raw }: { raw: string }) {
  const t = parseTrace(raw);
  // 一律默认折叠(含失败):多工具场景失败行也不抢占阅读流,点开即可看错误输出
  const [open, setOpen] = useState(false);
  const [viewingFull, setViewingFull] = useState(false);

  // 流式期间的残缺块:只留占位,不把半截 JSON 暴露给用户
  if (!t) {
    return (
      <div className="tool-trace">
        <div className="tool-row">
          <IconChevronRight size={12} />
          <IconTerminal size={12} />
          <span className="tname">tool</span>
          <span className="targ">工具操作进行中…</span>
        </div>
      </div>
    );
  }
  if (t.tool === 'delegate') return <SubagentCard trace={t} />;
  const Icon = toolIcon(t.tool);
  const elapsed = fmtMs(t.elapsedMs);
  const arg = argSummary(t);

  return (
    <div className={`tool-trace${open ? ' open' : ''}${t.ok ? '' : ' err'}`}>
      <button
        type="button"
        className={`tool-row${open ? ' open' : ''}`}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        title={`${actionLabel(t.tool)}${t.server ? ` · ${t.server}` : ''}`}
      >
        {open ? <IconChevronDown size={12} /> : <IconChevronRight size={12} />}
        <Icon size={12} />
        <span className="tname">{t.tool}</span>
        {arg && <span className="targ">{arg}</span>}
        <span className="tsp" />
        {elapsed && <span className="ttime">{elapsed}</span>}
        <span className={`tstat ${t.ok ? 'ok' : 'err'}`}>
          {t.ok ? '成功' : '失败'}
        </span>
      </button>
      {open && (
        <>
          <pre className={`tool-out${viewingFull ? ' full' : ''}`}>{t.content || '(无输出)'}</pre>
          <div className="tool-actions">
            <button type="button" className="act" onClick={() => setOpen(false)}>
              <IconRefresh size={12} />
              重试
            </button>
            <button
              type="button"
              className="act"
              onClick={() => setViewingFull((v) => !v)}
              disabled={!t.content}
            >
              <IconFileText size={12} />
              {viewingFull ? '收起完整输出' : '查看完整输出'}
            </button>
            <span className="sp" />
            <button type="button" className="act" disabled={!t.content}>
              <IconSkipForward size={12} />
              跳过此步
            </button>
          </div>
        </>
      )}
    </div>
  );
}
