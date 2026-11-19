import type { AttachmentEntry } from '../../lib/ipc';
import { KIND_LABEL, KindIcon, fmtDate, fmtSize } from '../../pages/attachments/shared';

interface Props {
  entry: AttachmentEntry;
}

/** 预览摘要优先级:已生成摘要 > 解析失败原因 > 提取正文前 400 字 > 缺省提示 */
function summaryOf(entry: AttachmentEntry): string {
  const m = entry.meta;
  if (m.summary) return m.summary;
  if (m.extract_note) return m.extract_note;
  if (m.extracted_text) return m.extracted_text.slice(0, 400);
  if (m.kind === 'image') return '图片附件不提取文本,导入后可直接复用。';
  return '该附件尚未解析,导入后将重新提取。';
}

/**
 * 附件中心导入选择器的右侧预览栏。
 * 只做展示,不参与勾选状态;窄容器下由容器查询整体隐藏。
 */
export default function AttachmentPreviewPane({ entry }: Props) {
  const m = entry.meta;
  const chunks = m.chunk_count ?? 0;
  const reusable = m.kind === 'image' || chunks > 0;
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
          </div>
        </div>
      </div>
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
      <div className="att-prev-section">
        <div className="att-prev-label">解析摘要</div>
        <p className="att-prev-summary">{summaryOf(entry)}</p>
      </div>
    </aside>
  );
}
