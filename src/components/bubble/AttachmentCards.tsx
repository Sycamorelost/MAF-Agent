import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import type { AttachmentMeta } from '../../lib/ipc';
import { IconDoc } from '../icons';

/** 单张图片附件:按需读 data URL 渲染缩略图,点击放大。 */
function ImageAttachment({ meta }: { meta: AttachmentMeta }) {
  const [url, setUrl] = useState<string | null>(null);
  const [zoom, setZoom] = useState(false);
  useEffect(() => {
    let alive = true;
    ipc
      .readAttachmentData(meta.path)
      .then((u) => alive && setUrl(u))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [meta.path]);
  if (!url) return <div className="attach-card">加载图片…</div>;
  return (
    <>
      <img
        className="attach-thumb"
        src={url}
        alt={meta.name}
        title={meta.name}
        onClick={() => setZoom(true)}
      />
      {zoom && (
        <div className="draw-lightbox" onClick={() => setZoom(false)}>
          <img src={url} alt={meta.name} />
        </div>
      )}
    </>
  );
}

/** 附件卡列表(渲染在气泡上方)。 */
export function AttachmentCards({ attachments }: { attachments: string | null }) {
  if (!attachments) return null;
  let metas: AttachmentMeta[] = [];
  try {
    metas = JSON.parse(attachments);
  } catch {
    return null;
  }
  if (metas.length === 0) return null;
  return (
    <div className="attach-cards">
      {metas.map((m, i) =>
        m.kind === 'image' ? (
          <ImageAttachment key={`${m.path}-${i}`} meta={m} />
        ) : (
          <div key={`${m.path}-${i}`} className="attach-card" title={m.path}>
            <IconDoc size={16} />
            <div className="attach-card-info">
              <div className="attach-card-name">{m.name}</div>
              <div className="attach-card-kind">
                {m.kind.toUpperCase()}
                {m.extracted_text ? ' · 已解析文本' : ''}
              </div>
            </div>
          </div>
        ),
      )}
    </div>
  );
}
