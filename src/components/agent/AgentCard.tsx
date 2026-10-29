import * as ipc from '../../lib/ipc';
import { agentIconComponent } from '../icons';

export const CAT_LABEL: Record<string, string> = {
  chat: '日常',
  doc: '文档',
  dev: '开发',
  create: '创作',
  ops: '运维',
  meta: '管理',
  life: '生活',
  study: '学习',
  work: '工作',
  academic: '学术',
};

/** 分类顺序按使用频率排:日常 → 文档(含去 AI 味) → 学术 → 工作 → 学习 → 开发 → 创作 → 生活 → 运维 → 管理。 */
export const CAT_ORDER = ['chat', 'doc', 'academic', 'work', 'study', 'dev', 'create', 'life', 'ops', 'meta'];

/** 可选图标 key(与 agentIconComponent 的映射一致,禁 emoji)。 */
export const ICON_KEYS = [
  'bot', 'sparkles', 'languages', 'clipboard-list', 'file-text', 'git-compare',
  'git-pull-request', 'refresh-cw', 'bug', 'search', 'wand-sparkles', 'shield-check',
  'archive', 'chat', 'note', 'brain', 'globe', 'settings', 'image', 'translate',
  'research', 'attach', 'doc', 'check', 'edit',
];

/** 完整智能体表单状态(新建与编辑共用)。 */
export interface EditorState {
  mode: 'new' | 'edit';
  agent_id: string;
  name: string;
  icon: string;
  description: string;
  category: string;
  recommended_model: string;
  memory_strategy: ipc.AgentTemplate['memory_strategy'];
  system_prompt: string;
  /** 一行一条 */
  input_hints: string;
  memory_write: boolean;
  /** 一行一节 */
  memory_sections: string;
  /** 编辑变体时保留基座 id,避免编辑后丢失「变体」属性 */
  base_agent: string | null;
}

export function emptyEditor(): EditorState {
  return {
    mode: 'new',
    agent_id: '',
    name: '',
    icon: 'bot',
    description: '',
    category: 'chat',
    recommended_model: '',
    memory_strategy: 'full',
    system_prompt: '',
    input_hints: '',
    memory_write: true,
    memory_sections: '会话摘要\n关键事实与结论\n待办',
    base_agent: null,
  };
}

export function toLines(s: string): string[] {
  return s
    .split('\n')
    .map((x) => x.trim())
    .filter(Boolean);
}

interface CardProps {
  t: ipc.AgentTemplate;
  isActive: boolean;
  sessionId?: string | null;
  onSelect: (agentId: string) => void;
  onApplyPrompt: (agentId: string) => void;
  onCreateVariant: (t: ipc.AgentTemplate) => void;
  onEdit: (t: ipc.AgentTemplate) => void;
  onRemove: (agentId: string) => void;
}

/** 智能体卡片:绑定 / 应用为提示词 / 创建变体 / 编辑 / 删除。 */
export function AgentCard({
  t,
  isActive,
  sessionId,
  onSelect,
  onApplyPrompt,
  onCreateVariant,
  onEdit,
  onRemove,
}: CardProps) {
  const Icon = agentIconComponent(t.icon);
  return (
    <div className={`agent-card ${isActive ? 'active' : ''}`} onClick={() => onSelect(t.agent_id)}>
      <div className="agent-card-head">
        <Icon size={18} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="agent-card-name">{t.name}</div>
          <div className="agent-card-desc">{t.description}</div>
        </div>
        {isActive && <span className="agent-card-badge" style={{ background: 'var(--ok)' }}>当前</span>}
        {t.base_agent && <span className="agent-card-badge">变体</span>}
      </div>
      <div className="agent-card-meta">
        <span>模型 {t.recommended_model}</span>
        <span>·</span>
        <span>{t.memory_strategy === 'none' ? '不注入记忆' : `注入 ${t.memory_strategy}`}</span>
        {t.memory_write ? <span className="agent-card-ok">· 自动提炼</span> : <span>· 不提炼</span>}
      </div>
      <div className="agent-card-actions">
        {sessionId && (
          <button
            type="button"
            className="btn ghost micro"
            title="不绑定 Agent,仅把渲染后的提示词写入会话设置"
            onClick={(e) => {
              e.stopPropagation();
              void onApplyPrompt(t.agent_id);
            }}
          >
            应用为提示词
          </button>
        )}
        {t.builtin && (
          <button
            type="button"
            className="btn ghost micro"
            onClick={(e) => {
              e.stopPropagation();
              onCreateVariant(t);
            }}
          >
            创建变体
          </button>
        )}
        {!t.builtin && (
          <>
            <button
              type="button"
              className="btn ghost micro"
              onClick={(e) => {
                e.stopPropagation();
                onEdit(t);
              }}
            >
              编辑
            </button>
            <button
              type="button"
              className="btn ghost micro"
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(`删除自定义模板「${t.name}」?`)) onRemove(t.agent_id);
              }}
            >
              删除
            </button>
          </>
        )}
      </div>
    </div>
  );
}

interface EditorFormProps {
  editor: EditorState;
  saving: boolean;
  onChange: (next: EditorState) => void;
  onCancel: () => void;
  onSubmit: (e: React.FormEvent) => void;
}

/** 完整智能体编辑器(新建 / 编辑自定义模板)。 */
export function AgentEditorForm({ editor, saving, onChange, onCancel, onSubmit }: EditorFormProps) {
  return (
    <div style={{ marginTop: 'var(--space-4)', borderTop: '1px solid var(--border)', paddingTop: 'var(--space-4)' }}>
      <div style={{ fontWeight: 700, marginBottom: 'var(--space-2)' }}>
        {editor.mode === 'edit' ? '编辑智能体' : '新建智能体'}
      </div>
      <form onSubmit={onSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
        <div className="agent-form-row">
          <div>
            <label className="lbl">名称 *</label>
            <input
              className="input"
              value={editor.name}
              onChange={(e) => onChange({ ...editor, name: e.target.value })}
              placeholder="如:简历优化助手"
            />
          </div>
          <div>
            <label className="lbl">分类</label>
            <select
              className="input"
              value={editor.category}
              onChange={(e) => onChange({ ...editor, category: e.target.value })}
            >
              {CAT_ORDER.map((c) => (
                <option key={c} value={c}>
                  {CAT_LABEL[c]}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="agent-form-row">
          <div>
            <label className="lbl">图标</label>
            <select
              className="input"
              value={editor.icon}
              onChange={(e) => onChange({ ...editor, icon: e.target.value })}
            >
              {ICON_KEYS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="lbl">推荐模型</label>
            <input
              className="input"
              value={editor.recommended_model}
              onChange={(e) => onChange({ ...editor, recommended_model: e.target.value })}
              placeholder="如 gpt-5.2"
            />
          </div>
        </div>
        <div>
          <label className="lbl">描述</label>
          <input
            className="input"
            value={editor.description}
            onChange={(e) => onChange({ ...editor, description: e.target.value })}
            placeholder="一句话说明这个智能体做什么"
          />
        </div>
        <div>
          <label className="lbl">记忆注入策略</label>
          <select
            className="input"
            value={editor.memory_strategy}
            onChange={(e) =>
              onChange({ ...editor, memory_strategy: e.target.value as ipc.AgentTemplate['memory_strategy'] })
            }
          >
            <option value="full">full — 注入完整记忆</option>
            <option value="summary">summary — 只注入摘要</option>
            <option value="none">none — 不注入记忆</option>
          </select>
        </div>
        <div>
          <label className="lbl">system_prompt *</label>
          <textarea
            className="input"
            rows={6}
            value={editor.system_prompt}
            onChange={(e) => onChange({ ...editor, system_prompt: e.target.value })}
            placeholder="角色 / 能力边界 / 输出格式 / 约束…(支持 {date} {model} {language} 变量)"
          />
        </div>
        <div>
          <label className="lbl">输入提示(一行一条)</label>
          <textarea
            className="input"
            rows={3}
            value={editor.input_hints}
            onChange={(e) => onChange({ ...editor, input_hints: e.target.value })}
            placeholder="帮我优化这段文字"
          />
        </div>
        <label className="agent-check">
          <input
            type="checkbox"
            checked={editor.memory_write}
            disabled={editor.memory_strategy === 'none'}
            onChange={(e) => onChange({ ...editor, memory_write: e.target.checked })}
          />
          参与记忆自动提炼(strategy=none 时不可选)
        </label>
        <div>
          <label className="lbl">写入记忆的节(一行一节)</label>
          <textarea
            className="input"
            rows={3}
            value={editor.memory_sections}
            onChange={(e) => onChange({ ...editor, memory_sections: e.target.value })}
            placeholder="会话摘要"
          />
        </div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
          <button type="button" className="btn ghost" onClick={onCancel}>取消</button>
          <button type="submit" className="btn primary" disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </form>
    </div>
  );
}
