//! 附件中心浮层:图片灯箱、文本预览、批量清理确认弹窗。

import { IconX } from '../../components/icons';

/** 图片灯箱:点击任意处关闭。 */
export function ImageLightbox({ src, onClose }: { src: string; onClose: () => void }) {
  return (
    <div className="att-lightbox" onClick={onClose}>
      <img src={src} alt="预览" />
    </div>
  );
}

/** 文本预览:超长提取文本在弹窗内滚动。 */
export function TextPreviewModal({
  name,
  text,
  onClose,
}: {
  name: string;
  text: string;
  onClose: () => void;
}) {
  return (
    <div className="att-modal-mask" onClick={onClose}>
      <div className="att-modal" onClick={(ev) => ev.stopPropagation()}>
        <div className="att-modal-head">
          <span className="t" title={name}>
            {name}
          </span>
          <button onClick={onClose} title="关闭">
            <IconX size={14} />
          </button>
        </div>
        <pre className="att-modal-body">{text}</pre>
      </div>
    </div>
  );
}

/** 批量清理确认:清理期间禁止关闭,避免误操作后无法看到结果。 */
export function ConfirmBatchModal({
  count,
  deleting,
  onCancel,
  onConfirm,
}: {
  count: number;
  deleting: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      className="att-modal-mask"
      onClick={() => {
        if (!deleting) onCancel();
      }}
    >
      <div className="att-modal att-confirm" onClick={(ev) => ev.stopPropagation()}>
        <div className="att-modal-head">
          <span className="t">批量清理确认</span>
          <button onClick={onCancel} disabled={deleting} title="关闭">
            <IconX size={14} />
          </button>
        </div>
        <div className="att-confirm-body">
          <p>
            将删除 <b>{count}</b> 个附件引用;磁盘文件在无其他引用时一并清理。
          </p>
          <p className="hint">删除后不可恢复,可分批操作,失败的条目会保留在列表中。</p>
        </div>
        <div className="att-modal-foot">
          <span className="sp" />
          <button className="att-btn" onClick={onCancel} disabled={deleting}>
            取消
          </button>
          <button className="att-btn danger" onClick={onConfirm} disabled={deleting}>
            {deleting ? '清理中…' : `确认删除 ${count} 个`}
          </button>
        </div>
      </div>
    </div>
  );
}
