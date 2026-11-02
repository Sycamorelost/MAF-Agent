import type { AgentTemplate } from '../../lib/ipc';
import { IconChat } from '../../components/icons';

interface ChatEmptyStateProps {
  agent: AgentTemplate | null;
  /** 点击建议 chip 时填入输入框 */
  onPickSuggestion: (text: string) => void;
}

/** 空会话占位:智能体开场白 + 追问建议。 */
export function ChatEmptyState({ agent, onPickSuggestion }: ChatEmptyStateProps) {
  const opening = agent?.opening_message?.trim();
  return (
    <div className="main-empty">
      <div style={{ color: 'var(--text-4)' }}>
        <IconChat size={48} strokeWidth={1} />
      </div>
      <div>{opening || '开始你的第一段对话吧'}</div>
      {!!agent?.suggestions?.length && (
        <div className="main-empty-sugs">
          {agent.suggestions.map((s) => (
            <button
              key={s}
              type="button"
              className="main-empty-sug"
              onClick={() => onPickSuggestion(s)}
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
