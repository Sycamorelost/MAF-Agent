import type { AttachmentMeta } from '../../lib/ipc';
import { IconDoc, IconImage, IconX } from '../icons';

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / 1024 / 1024).toFixed(1)}M`;
}

interface Props {
  attachments: AttachmentMeta[];
  onRemoveAttachment?: (index: number) => void;
}

/** 待发送附件 chip 列表(图标 / 名称 / 大小 / 移除)。 */
export default function AttachTray({ attachments, onRemoveAttachment }: Props) {
  return (
    <div className="attach-list">
      {attachments.map((a, i) => (
        <span key={`${a.path}-${i}`} className="attach-chip" title={a.name}>
          {a.kind === 'image' ? <IconImage size={13} /> : <IconDoc size={13} />}
          <span className="attach-name">{a.name}</span>
          <span className="attach-size">{fmtSize(a.size)}</span>
          {onRemoveAttachment && (
            <button
              type="button"
              className="attach-x"
              onClick={() => onRemoveAttachment(i)}
              title="移除附件"
            >
              <IconX size={11} />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}
