import {
  AgentEditorForm,
  type EditorState,
} from '../../components/agent/AgentCard';

interface AgentEditorDialogProps {
  editor: EditorState;
  saving: boolean;
  advanced: boolean;
  availableModels: string[];
  onToggleAdvanced: (next: boolean) => void;
  onChange: (next: EditorState) => void;
  onClose: () => void;
  onSubmit: (e: React.FormEvent) => void;
}

/** 智能体新建 / 编辑弹窗:高级模式开关 + 完整表单。 */
export function AgentEditorDialog({
  editor,
  saving,
  advanced,
  availableModels,
  onToggleAdvanced,
  onChange,
  onClose,
  onSubmit,
}: AgentEditorDialogProps) {
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(calc(560px * var(--ui-scale)), 92vw)', maxHeight: '82vh', display: 'flex', flexDirection: 'column' }}
      >
        <div className="dialog-head">
          {editor.mode === 'edit' ? '编辑智能体' : '新建智能体'}
        </div>
        <div className="dialog-body" style={{ overflow: 'auto', flex: 1 }}>
          <label className="agent-check" style={{ marginBottom: 'var(--space-3)' }}>
            <input
              type="checkbox"
              checked={advanced}
              onChange={(e) => onToggleAdvanced(e.target.checked)}
            />
            高级模式(展示输入提示、记忆提炼等高级字段)
          </label>
          <AgentEditorForm
            editor={editor}
            saving={saving}
            onChange={onChange}
            onCancel={onClose}
            onSubmit={onSubmit}
            advanced={advanced}
            availableModels={availableModels}
          />
        </div>
      </div>
    </div>
  );
}
