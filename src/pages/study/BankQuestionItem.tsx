/**
 * 题库列表行:勾选组卷 + 题干/元信息/参考答案 + 编辑/删除。
 * 答案与解析在题库中可见,便于整理错题;做题时不预置。
 */
import { IconTrash } from '../../components/icons';
import type { BankQuestion } from '../../lib/ipc/study';
import { TYPE_LABEL } from './qtype';

export function BankQuestionItem({
  q,
  selected,
  onToggle,
  onEdit,
  onDelete,
}: {
  q: BankQuestion;
  selected: boolean;
  onToggle: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="rp-hist-item st-static">
      <input type="checkbox" className="st-mt-1" checked={selected} onChange={onToggle} title="选中后可组卷自测" />
      <div className="rp-hist-main">
        <div className="st-row-base">
          <span className={`mode-tag ${q.source === 'manual' ? 'staged' : 'legacy'}`}>
            {TYPE_LABEL[q.qtype] ?? q.qtype}
          </span>
          <b className="st-fs-caption">{q.stem}</b>
        </div>
        {q.options.length > 0 && <div className="pg-hint">{q.options.join('  ')}</div>}
        <div className="rp-hist-meta">
          {q.node_name && <span>{q.node_name}</span>}
          <span>难度 {q.difficulty}/5</span>
          <span>{q.source === 'manual' ? '手动加题' : '小测生成'}</span>
        </div>
        <div className="pg-hint">
          参考答案:{q.answer}
          {q.explanation ? ` · ${q.explanation}` : ''}
        </div>
      </div>
      <div className="st-actions">
        <button className="pg-chip" onClick={onEdit}>
          编辑
        </button>
        <button className="icon-btn" title="删除" onClick={onDelete}>
          <IconTrash size={14} />
        </button>
      </div>
    </div>
  );
}
