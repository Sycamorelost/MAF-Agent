import { memo, useEffect, useRef, useState } from 'react';
import { IconBrain, IconChevronDown } from './icons';

/** 思考 token 估算:CJK 约 1 字/token,ASCII 约 4 字符/token(仅用于无端点上报时的「约 N」展示)。 */
export function estimateReasoningTokens(text: string): number {
  if (!text) return 0;
  let cjk = 0;
  for (const ch of text) {
    const c = ch.codePointAt(0) ?? 0;
    if ((c >= 0x3000 && c <= 0x9fff) || (c >= 0xff00 && c <= 0xffef)) cjk += 1;
  }
  const other = text.length - cjk;
  return Math.max(1, cjk + Math.ceil(other / 4));
}

function fmtSecs(ms: number | null | undefined): string {
  if (ms == null || ms < 0) return '';
  return ms < 1000 ? '<1s' : `${Math.round(ms / 1000)}s`;
}

interface Props {
  /** 思考文本(空 = 仅计数模式) */
  text: string;
  /** 端点上报的思考 token(有则用精确值,否则估算) */
  tokens?: number | null;
  /** 思考耗时 ms(流式阶段结束定格;历史消息无) */
  elapsedMs?: number | null;
  /** 是否处于流式会话(true 且 text 为空时显示「思考中…」) */
  streaming?: boolean;
}

/** 思考折叠块:任何状态默认折叠,展开/收起纯手动(展开态不持久化,刷新恢复折叠)。 */
export default memo(function ReasoningBlock({
  text,
  tokens = null,
  elapsedMs = null,
  streaming = false,
}: Props) {
  const [open, setOpen] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  // 展开态下思考流继续增长:跟随滚动到底部
  useEffect(() => {
    if (open && streaming && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [open, streaming, text]);

  const countOnly = !text && tokens != null && tokens > 0;
  if (!text && (tokens == null || tokens <= 0)) return null; // 无思考无计数,不渲染

  const exact = tokens != null && tokens > 0;
  const shown = exact ? tokens : estimateReasoningTokens(text);
  const tok = `${exact ? '' : '约 '}${shown} tokens`;
  const secs = fmtSecs(elapsedMs);
  // 仅计数无明文思考(OpenAI 官方端点):展示消耗,不提供展开
  if (countOnly) {
    return (
      <div className="reasoning-block count-only">
        <div className="reasoning-head">
          <IconBrain size={14} />
          <b>思考 · 已含在输出内</b>
          <span className="sp" />
          <span className="reasoning-tok">{tok}</span>
        </div>
      </div>
    );
  }

  const state = open ? '已展开' : streaming && !text ? '进行中' : '已折叠';
  return (
    <div className={`reasoning-block${open ? ' open' : ''}`}>
      <button
        type="button"
        className="reasoning-head"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        title={open ? '收起思考过程' : '展开思考过程'}
      >
        {streaming && !text ? (
          <span className="reasoning-pulse" aria-hidden />
        ) : (
          <IconBrain size={14} />
        )}
        <b>思考 · {state}</b>
        <span className="sp" />
        {secs && <span className="reasoning-tok">{secs}</span>}
        <span className="reasoning-tok">{tok}</span>
        <IconChevronDown size={14} />
      </button>
      {open && (
        <div className="reasoning-body" ref={bodyRef}>
          {text || (streaming ? '…' : '(思考内容为空)')}
        </div>
      )}
    </div>
  );
});
