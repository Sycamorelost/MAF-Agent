import type { VersionItem } from '../../lib/ipc';
import type { DiffSide } from '../MemoryDiffDialog';

interface Props {
  versions: VersionItem[];
  selected: number[];
  onToggle: (versionId: number) => void;
  onOpenDiff: (left: DiffSide, right: DiffSide) => void;
  onRollback: (versionId: number) => void;
}

/** 记忆历史版本列表(勾选两条并排对比 / 与当前对比 / 回滚)。 */
export function VersionHistory({ versions, selected, onToggle, onOpenDiff, onRollback }: Props) {
  return (
    <div className="mem-versions">
      {versions.length === 0 && <div style={{ color: 'var(--text-4)' }}>暂无历史版本</div>}
      {versions.map((v) => {
        const isSelected = selected.includes(v.id);
        return (
          <div key={v.id} className="mem-version-row">
            <input
              type="checkbox"
              title="选中以对比"
              checked={isSelected}
              onChange={() => onToggle(v.id)}
            />
            <span className="mem-version-meta" style={{ color: 'var(--text-3)' }}>
              v{v.id} · {v.note}
            </span>
            <div className="mem-version-actions">
              <button
                type="button"
                className="btn ghost"
                onClick={() => onOpenDiff({ label: `v${v.id}`, version: v.id }, { label: '当前', version: null })}
              >
                与当前对比
              </button>
              <button
                type="button"
                className="btn ghost"
                onClick={() => {
                  if (confirm(`回滚到 v${v.id}(${v.note})?`)) onRollback(v.id);
                }}
              >
                回滚
              </button>
            </div>
          </div>
        );
      })}
      {selected.length === 2 && (
        <div className="mem-diff-bar">
          <button
            type="button"
            className="btn primary"
            onClick={() =>
              onOpenDiff(
                { label: `v${selected[0]}`, version: selected[0] },
                { label: `v${selected[1]}`, version: selected[1] },
              )
            }
          >
            并排对比
          </button>
        </div>
      )}
    </div>
  );
}
