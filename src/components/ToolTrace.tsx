//! 工具调用痕迹 —— 正文里一行说明「做了什么」,点开才展开完整输出(对标截图 3/4)。
//! 数据由 useChatSend 注入为 ```maf-tool 围栏块(单行 JSON),持久化后仍按此渲染。
//! 折叠行 24px 高;操作只保留可用的「查看完整输出(按 log_id 拉全文)/复制」;状态色仅 ok / err。

import { useState } from 'react';
import {
  IconChevronDown,
  IconChevronRight,
  IconCopy,
  IconFileText,
  IconLayers,
  IconSearch,
  IconTerminal,
} from './icons';
import { SubagentCard } from './SubagentCard';
import { actionLabel, argSummary, fmtMs, parseTrace } from '../lib/toolTrace';
import { mcpGetToolLog } from '../lib/ipc/mcp';

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
  // P4:完整输出按 log_id 按需拉取(而非把全文塞进历史消息)。
  const [full, setFull] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

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

  /** P4:按需拉取完整输出;再次点击则收起(只显示截断后的正文)。 */
  const toggleFull = async () => {
    if (full != null) {
      setFull(null);
      return;
    }
    if (!t.logId) return;
    setLoading(true);
    try {
      const row = await mcpGetToolLog(t.logId);
      setFull(row?.result ?? '');
    } catch {
      setFull('');
    } finally {
      setLoading(false);
    }
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(full ?? t.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 剪贴板不可用时静默(受系统权限限制) */
    }
  };

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
          <pre className={`tool-out${full != null ? ' full' : ''}`}>
            {(full ?? t.content) || '(无输出)'}
          </pre>
          <div className="tool-actions">
            <button
              type="button"
              className="act"
              onClick={() => void toggleFull()}
              disabled={!t.logId || loading}
              title={t.logId ? '按需加载完整输出' : '该记录未保存完整输出'}
            >
              <IconFileText size={12} />
              {loading ? '加载中…' : full != null ? '收起完整输出' : '查看完整输出'}
            </button>
            <span className="sp" />
            <button type="button" className="act" onClick={() => void copy()}>
              <IconCopy size={12} />
              {copied ? '已复制' : '复制'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
