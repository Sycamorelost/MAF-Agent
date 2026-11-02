import { MemoMarkdown } from '../MemoMarkdown';
import { bodyOf } from '../../lib/memFile';

interface Props {
  /** 磁盘最新内容(含 frontmatter);null = 正在加载 */
  incoming: string | null;
  onClose: () => void;
  onAdopt: () => void;
}

/** 记忆编辑冲突:展示磁盘最新版本,可继续编辑或采用新版本。 */
export function MemConflictDialog({ incoming, onClose, onAdopt }: Props) {
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(calc(520px * var(--ui-scale)), 92vw)' }}
      >
        <div className="dialog-head">新版本内容</div>
        <div className="dialog-body">
          <p
            style={{
              fontSize: 'var(--font-caption)',
              color: 'var(--text-3)',
              marginBottom: 'var(--space-3)',
            }}
          >
            文件已被外部程序或自动提炼更新,以下为新内容(采用后将放弃你未保存的编辑):
          </p>
          <div
            className="mem-view mem-preview"
            style={{ maxHeight: 'calc(320px * var(--ui-scale))', overflowY: 'auto' }}
          >
            {/* 统一排版:走全局 `.md` 模板 */}
            <div className="md">
              <MemoMarkdown content={incoming !== null ? bodyOf(incoming) : '加载中…'} />
            </div>
          </div>
          <div
            style={{
              display: 'flex',
              justifyContent: 'flex-end',
              gap: 'var(--space-2)',
              marginTop: 'var(--space-4)',
            }}
          >
            <button className="btn ghost" onClick={onClose}>
              继续我的编辑
            </button>
            <button className="btn primary" onClick={onAdopt}>
              采用新版本
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
