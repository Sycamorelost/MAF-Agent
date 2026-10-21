import { IconBrain, IconNote } from '../icons';

/** F9:光标前活跃的 @ 触发词(@ 须在行首/空白后,其后 ≥1 个非空白字符)。 */
export function findAtTrigger(text: string, cursor: number): { start: number; query: string } | null {
  let i = cursor - 1;
  while (i >= 0) {
    const ch = text[i];
    if (ch === '@') {
      if (i === 0 || /\s/.test(text[i - 1])) {
        const query = text.slice(i + 1, cursor);
        if (query.length >= 1) return { start: i, query };
      }
      return null;
    }
    if (/\s/.test(ch)) return null;
    i--;
  }
  return null;
}

/** F9 候选项:记忆(其他会话 memory.md 命中)或知识库笔记。 */
export interface AtItem {
  kind: 'memory' | 'note';
  key: string;
  title: string;
  snippet: string;
}

interface Props {
  query: string;
  items: AtItem[];
  active: number;
  onActive: (index: number) => void;
  onPick: (item: AtItem) => void;
}

/** F9 候选浮层(记忆 / 知识库分组展示)。 */
export default function AtMenu({ query, items, active, onActive, onPick }: Props) {
  return (
    <div className="at-dropdown">
      {items.length === 0 ? (
        <div className="gs-item empty">没有匹配「{query}」的记忆/笔记</div>
      ) : (
        items.map((it, i) => (
          <div key={it.key}>
            {(i === 0 || it.kind !== items[i - 1].kind) && (
              <div className="gs-group-label">{it.kind === 'memory' ? '记忆' : '知识库'}</div>
            )}
            <div
              className={`gs-item ${i === active ? 'active' : ''}`}
              onMouseEnter={() => onActive(i)}
              onClick={() => onPick(it)}
            >
              <div className="gs-item-row">
                {it.kind === 'memory' ? (
                  <IconBrain size={14} className="gs-item-icon" />
                ) : (
                  <IconNote size={14} className="gs-item-icon" />
                )}
                <div className="gs-item-title">{it.title}</div>
              </div>
              {it.snippet && <div className="gs-item-snippet">{it.snippet}</div>}
            </div>
          </div>
        ))
      )}
    </div>
  );
}
