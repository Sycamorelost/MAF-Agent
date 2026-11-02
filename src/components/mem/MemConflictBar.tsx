interface Props {
  onView: () => void;
  onDiscard: () => void;
}

/** 记忆编辑冲突提示条:外部变更时的「查看 / 丢弃」。 */
export function MemConflictBar({ onView, onDiscard }: Props) {
  return (
    <div className="mem-conflict-bar">
      <span>检测到外部变更/新提炼结果</span>
      <div className="mem-conflict-actions">
        <button type="button" className="btn primary" onClick={onView}>
          查看
        </button>
        <button type="button" className="btn ghost" onClick={onDiscard}>
          丢弃
        </button>
      </div>
    </div>
  );
}
