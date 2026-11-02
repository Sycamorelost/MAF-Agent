//! 附件卡片:缩略图 / 名称 / 元信息 / 操作(预览、插入、定位、删除、重试提取)。
//! 批量模式下左上角显示勾选控件,点击卡片主体即切换选中。

import type { AttachmentEntry } from '../../lib/ipc';
import {
  IconCheck,
  IconExternalLink,
  IconEye,
  IconInfo,
  IconPlus,
  IconRefresh,
  IconTrash,
} from '../../components/icons';
import { KIND_LABEL, KindIcon, canRetryExtract, delKey, fmtDate, fmtSize } from './shared';

export interface AttachmentCardProps {
  entry: AttachmentEntry;
  thumb?: string;
  selectMode: boolean;
  selected: boolean;
  confirming: boolean;
  retrying: boolean;
  onPreview: (e: AttachmentEntry) => void;
  onInsert: (e: AttachmentEntry) => void;
  onLocate: (e: AttachmentEntry) => void;
  onDelete: (e: AttachmentEntry) => void;
  onToggleSelect: (e: AttachmentEntry) => void;
  onRetry: (e: AttachmentEntry) => void;
}

export function AttachmentCard({
  entry: e,
  thumb,
  selectMode,
  selected,
  confirming,
  retrying,
  onPreview,
  onInsert,
  onLocate,
  onDelete,
  onToggleSelect,
  onRetry,
}: AttachmentCardProps) {
  const canPreview = e.meta.kind === 'image' || !!e.meta.extracted_text;
  const retryable = canRetryExtract(e);
  return (
    <div
      className={`att-card${selected ? ' selected' : ''}`}
      onClick={() => selectMode && onToggleSelect(e)}
    >
      {selectMode && (
        <button
          className={`att-select${selected ? ' on' : ''}`}
          title={selected ? '取消选择' : '选择此项'}
          onClick={(ev) => {
            ev.stopPropagation();
            onToggleSelect(e);
          }}
        >
          {selected && <IconCheck size={12} strokeWidth={2} />}
        </button>
      )}
      <div
        className={`att-thumb ${canPreview && !selectMode ? 'clickable' : ''}`}
        onClick={() => !selectMode && canPreview && onPreview(e)}
        title={canPreview ? '预览' : e.meta.name}
      >
        {thumb ? <img src={thumb} alt={e.meta.name} /> : <KindIcon kind={e.meta.kind} />}
      </div>
      <div className="att-name" title={e.meta.name}>
        {e.meta.name}
      </div>
      <div className="att-meta">
        {KIND_LABEL[e.meta.kind]} · {fmtSize(e.meta.size)} · {fmtDate(e.created)}
      </div>
      {retryable && (
        <div className="att-warn" title={e.meta.extract_note ?? undefined}>
          <IconInfo size={12} />
          <span className="t">{e.meta.extract_note}</span>
          <button
            className="att-retry"
            title={retrying ? '正在重试…' : '重试文本提取'}
            disabled={retrying}
            onClick={(ev) => {
              ev.stopPropagation();
              onRetry(e);
            }}
          >
            <IconRefresh size={12} />
          </button>
        </div>
      )}
      {!selectMode && (
        <div className="att-ops">
          {canPreview && (
            <button title="预览" onClick={() => onPreview(e)}>
              <IconEye size={14} />
            </button>
          )}
          <button title="插入当前会话" onClick={() => onInsert(e)}>
            <IconPlus size={14} />
          </button>
          <button title="定位来源消息" onClick={() => onLocate(e)}>
            <IconExternalLink size={14} />
          </button>
          <button
            className={confirming ? 'danger' : ''}
            title={confirming ? '再次点击确认删除' : '删除'}
            onClick={() => onDelete(e)}
            data-key={delKey(e)}
          >
            {confirming ? <IconCheck size={14} /> : <IconTrash size={14} />}
          </button>
        </div>
      )}
    </div>
  );
}
