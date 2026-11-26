import { useEffect, useState } from 'react';
import { readAttachmentData, type AttachmentEntry } from '../../lib/ipc';
import { KIND_LABEL, KindIcon, fmtDate, fmtSize } from '../../pages/attachments/shared';

interface Props {
  entry: AttachmentEntry;
}

/** 正文预览最多展示的字符数(超过则提示已截断) */
const TEXT_PREVIEW_CHARS = 1200;

/**
 * 附件中心导入选择器的右侧预览栏。
 * 先给「大致内容」:图片直接内联缩略图,文档给正文开头;
 * 元数据与解析摘要作为补充信息。只做展示,不参与勾选状态。
 */
export default function AttachmentPreviewPane({ entry }: Props) {
  const m = entry.meta;
  const chunks = m.chunk_count ?? 0;
  const reusable = m.kind === 'image' || chunks > 0;
  const isImage = m.kind === 'image';

  // 图片缩略图:按需读取为 data URL;切换条目时先清空,避免闪回上一张
  const [thumb, setThumb] = useState<string | null>(null);
  const [thumbFailed, setThumbFailed] = useState(false);
  useEffect(() => {
    if (!isImage) return;
    let alive = true;
    setThumb(null);
    setThumbFailed(false);
    readAttachmentData(m.path)
      .then((url) => {
        if (alive) setThumb(url);
      })
      .catch(() => {
        if (alive) setThumbFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [isImage, m.path]);

  const text = (m.extracted_text ?? '').trim();
  const clipped = text.length > TEXT_PREVIEW_CHARS;

  return (
    <aside className="att-pick-preview" aria-label="附件预览">
      <div className="att-prev-head">
        <span className="att-prev-icon">
          <KindIcon kind={m.kind} />
        </span>
        <div className="att-prev-title" title={m.name}>
          <div className="att-prev-name">{m.name}</div>
          <div className="att-prev-sub">
            {KIND_LABEL[m.kind]} · {fmtSize(m.size)}
            {text && ` · ${text.length} 字`}
          </div>
        </div>
      </div>

      {isImage && (
        <div className="att-prev-thumb">
          {thumb ? (
            <img src={thumb} alt={m.name} />
          ) : (
            <span className="att-prev-thumb-fail">
              {thumbFailed ? '图片读取失败' : '加载预览…'}
            </span>
          )}
        </div>
      )}

      {text && (
        <div className="att-prev-section">
          <div className="att-prev-label">内容预览</div>
          <pre className="att-prev-text">{text.slice(0, TEXT_PREVIEW_CHARS)}</pre>
          {clipped && (
            <div className="att-prev-more">
              仅显示前 {TEXT_PREVIEW_CHARS} 字,共 {text.length} 字
            </div>
          )}
        </div>
      )}

      {!text && m.summary && (
        <div className="att-prev-section">
          <div className="att-prev-label">摘要</div>
          <p className="att-prev-summary">{m.summary}</p>
        </div>
      )}

      {!text && !m.summary && (
        <div className="att-prev-section">
          <div className="att-prev-label">{isImage ? '说明' : '解析说明'}</div>
          <p className="att-prev-summary">
            {m.extract_note ??
              (isImage ? '图片附件不提取文本,导入后可直接复用。' : '该附件尚未解析,导入后将重新提取。')}
          </p>
        </div>
      )}

      <dl className="att-prev-fields">
        <dt>来源</dt>
        <dd title={entry.session_title}>{entry.session_title || '未知会话'}</dd>
        <dt>解析</dt>
        <dd>
          {chunks > 0 ? `已入库 ${chunks} 块` : reusable ? '无需解析' : '未解析'}
          {!reusable && <span className="att-prev-warn"> · 无法复用</span>}
        </dd>
        <dt>入库</dt>
        <dd>{fmtDate(entry.created)}</dd>
      </dl>
    </aside>
  );
}
