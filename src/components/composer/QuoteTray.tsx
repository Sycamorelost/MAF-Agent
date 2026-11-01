import type { QuoteRef } from '../../lib/quote';
import { quoteSourceLabel } from '../../lib/quote';
import { IconQuote, IconX } from '../icons';

interface Props {
  quotes: QuoteRef[];
  onRemoveQuote?: (id: string) => void;
}

/** 待发送引用条:引用 · 角色 · 时间 · 字数 + 单行摘要(完整原文留在 state)。
 *  与附件 tray 同层,不占用输入框正文。最多展示 3 条,其余折叠成「+N」。 */
export default function QuoteTray({ quotes, onRemoveQuote }: Props) {
  if (quotes.length === 0) return null;
  const shown = quotes.slice(0, 3);
  const rest = quotes.length - shown.length;
  return (
    <div className="quote-tray">
      {shown.map((q) => (
        <span key={q.id} className="quote-chip" title={q.text}>
          <IconQuote size={12} />
          <span className="quote-meta">{quoteSourceLabel(q)}</span>
          <span className="quote-excerpt">{q.excerpt}</span>
          {onRemoveQuote && (
            <button
              type="button"
              className="quote-x"
              onClick={() => onRemoveQuote(q.id)}
              title="移除引用"
            >
              <IconX size={11} />
            </button>
          )}
        </span>
      ))}
      {rest > 0 && <span className="quote-more">+{rest}</span>}
    </div>
  );
}
