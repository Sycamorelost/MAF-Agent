import { useEffect } from 'react';
import { useResearchStore } from '../../stores/research';
import { IconDoc, IconTrash } from '../../components/icons';
import { STATUS_LABEL } from './constants';
import { fmtDate } from './helpers';

export function HistoryView({ onOpenLibrary }: { onOpenLibrary: () => void }) {
  const { list, refreshList, openDetail, remove } = useResearchStore();

  useEffect(() => {
    refreshList();
  }, [refreshList]);

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">研究历史</span>
        <button onClick={onOpenLibrary} title="参数 + 阶段编排 + 提示词 + 约束,一体保存复用">
          <IconDoc size={14} />
          方案库
        </button>
      </div>
      <div className="rp-hist-list">
        {list.length === 0 && <div className="rp-empty">还没有研究记录</div>}
        {list.map((r) => (
          <div key={r.id} className="rp-hist-item" onClick={() => openDetail(r.id)}>
            <span className={`stat-dot ${r.status === 'done' ? 'ok' : r.status === 'error' ? 'err' : r.status === 'cancelled' ? 'warn' : 'info'}`} />
            <div className="rp-hist-main">
              <div className="rp-hist-topic ellipsis">{r.topic}</div>
              <div className="rp-hist-meta">
                <span
                  className={`mode-tag ${
                    r.mode === 'staged' ? 'staged' : r.mode === 'concise' ? 'concise' : 'legacy'
                  }`}
                >
                  {r.mode === 'staged' ? '专项' : r.mode === 'concise' ? '简洁' : '快速'}
                </span>
                {r.mode === 'staged' && r.stages_total > 0 && (
                  <span className="hi-stages">
                    {r.stages_done}/{r.stages_total} 阶段
                  </span>
                )}
                {r.blueprint_name && <span className="hi-bp ellipsis">{r.blueprint_name}</span>}
                {fmtDate(r.created)} · {r.rounds}/{r.max_rounds} 轮 ·{' '}
                {STATUS_LABEL[r.status] ?? r.status}
              </div>
            </div>
            <button
              className="icon-btn"
              title="删除"
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(`删除研究「${r.topic}」?`)) void remove(r.id);
              }}
            >
              <IconTrash size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
