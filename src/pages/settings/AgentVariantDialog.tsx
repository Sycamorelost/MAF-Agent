import * as ipc from '../../lib/ipc';

/** 变体创建表单状态。 */
export interface VariantDraft {
  baseId: string;
  name: string;
  model: string;
  strategy: ipc.AgentTemplate['memory_strategy'];
  suffix: string;
}

interface AgentVariantDialogProps {
  variant: VariantDraft;
  availableModels: string[];
  saving: boolean;
  onChange: (next: VariantDraft) => void;
  onClose: () => void;
  onSubmit: (e: React.FormEvent) => void;
}

/** 创建变体弹窗:基于已有智能体另存为自定义配置。 */
export function AgentVariantDialog({
  variant,
  availableModels,
  saving,
  onChange,
  onClose,
  onSubmit,
}: AgentVariantDialogProps) {
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(calc(520px * var(--ui-scale)), 92vw)' }}
      >
        <div className="dialog-head">创建变体</div>
        <div className="dialog-body">
          <form onSubmit={onSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            <div>
              <label className="lbl">名称</label>
              <input
                className="input"
                value={variant.name}
                onChange={(e) => onChange({ ...variant, name: e.target.value })}
                placeholder="如:产品助手-深度版"
              />
            </div>
            <div>
              <label className="lbl">推荐模型</label>
              {availableModels.length > 0 ? (
                <select
                  className="input"
                  value={variant.model}
                  onChange={(e) => onChange({ ...variant, model: e.target.value })}
                >
                  <option value="">跟随全局当前模型</option>
                  {availableModels.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  className="input"
                  value={variant.model}
                  onChange={(e) => onChange({ ...variant, model: e.target.value })}
                  placeholder="如 gpt-5.2"
                />
              )}
            </div>
            <div>
              <label className="lbl">记忆注入策略</label>
              <select
                className="input"
                value={variant.strategy}
                onChange={(e) =>
                  onChange({ ...variant, strategy: e.target.value as ipc.AgentTemplate['memory_strategy'] })
                }
              >
                <option value="full">full — 注入完整记忆</option>
                <option value="summary">summary — 只注入摘要</option>
                <option value="none">none — 不注入记忆</option>
              </select>
            </div>
            <div>
              <label className="lbl">system_prompt 追加后缀</label>
              <textarea
                className="input"
                rows={3}
                value={variant.suffix}
                onChange={(e) => onChange({ ...variant, suffix: e.target.value })}
                placeholder="追加到原 system prompt 末尾的约束/风格…"
              />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
              <button type="button" className="btn ghost" onClick={onClose}>
                取消
              </button>
              <button type="submit" className="btn primary" disabled={saving}>
                {saving ? '保存中…' : '保存变体'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
