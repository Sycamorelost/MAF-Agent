import { useState } from 'react';
import type { SourceItem } from '../../lib/ipc';
import { sanitizeHttpUrl } from '../../lib/url';
import { IconBrain, IconChevronDown, IconChevronRight } from '../icons';

/** 联网来源卡片列表(渲染在气泡下方)。默认折叠、小字号:来源是辅助信息,不占主体版面。 */
export function SourceCards({
  raw,
  onSaveSources,
}: {
  raw: string | null;
  onSaveSources?: (sources: SourceItem[]) => void;
}) {
  const [saved, setSaved] = useState(false);
  const [open, setOpen] = useState(false);
  if (!raw) return null;
  let sources: SourceItem[] = [];
  try {
    sources = JSON.parse(raw);
  } catch {
    return null;
  }
  if (sources.length === 0) return null;
  const domain = (url: string) => {
    try {
      return new URL(url).hostname;
    } catch {
      return url;
    }
  };
  return (
    <div className={open ? 'src-cards open' : 'src-cards'}>
      <div className="src-cards-head">
        <button
          type="button"
          className="src-toggle"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          title={open ? '收起来源' : '展开来源'}
        >
          {open ? <IconChevronDown size={11} /> : <IconChevronRight size={11} />}
          <span>来源 ({sources.length})</span>
        </button>
        {open && onSaveSources && (
          <button
            type="button"
            className="src-save"
            disabled={saved}
            onClick={() => {
              onSaveSources(sources);
              setSaved(true);
            }}
            title="将来源摘要写入 memory.md 资料节"
          >
            <IconBrain size={12} />
            <span>{saved ? '已存入' : '存入记忆'}</span>
          </button>
        )}
      </div>
      {open &&
        sources.map((s) => {
          const safeUrl = sanitizeHttpUrl(s.url);
          if (!safeUrl) {
            return (
              <div key={s.id} className="src-card" title="链接不安全或格式非法,已屏蔽">
                <span className="src-id">[{s.id}]</span>
                <span className="src-title">{s.title || '无效链接'}</span>
                <span className="src-domain">已屏蔽</span>
              </div>
            );
          }
          return (
            <a
              key={s.id}
              className="src-card"
              href={safeUrl}
              target="_blank"
              rel="noreferrer"
              title={s.snippet}
            >
              <span className="src-id">[{s.id}]</span>
              <span className="src-title">{s.title || s.url}</span>
              <span className="src-domain">{domain(safeUrl)}</span>
            </a>
          );
        })}
    </div>
  );
}
