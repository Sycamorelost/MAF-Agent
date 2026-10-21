import { useEffect, useMemo, useState } from 'react';
import { useAgentsStore } from '../stores/agents';
import { useSessionsStore } from '../stores/sessions';
import { agentIconComponent } from './icons';
import * as ipc from '../lib/ipc';

interface Props {
  /** 当前会话已绑定的 agent_id(null = 空白) */
  currentAgentId?: string | null;
  /** 选中后回调;id=null 表示空白会话 */
  onSelect: (agentId: string | null) => void;
  onClose: () => void;
  /** B4 / F2:存在时卡片显示「应用为提示词」(仅写会话 system_prompt,不绑定 Agent) */
  sessionId?: string | null;
}

const CAT_LABEL: Record<string, string> = {
  chat: '日常',
  doc: '文档',
  dev: '开发',
  create: '创作',
  ops: '运维',
  meta: '管理',
};

const CAT_ORDER = ['chat', 'doc', 'dev', 'create', 'ops', 'meta'];

/** 可选图标 key(与 agentIconComponent 的映射一致,禁 emoji)。 */
const ICON_KEYS = [
  'bot', 'sparkles', 'languages', 'clipboard-list', 'file-text', 'git-compare',
  'git-pull-request', 'refresh-cw', 'bug', 'search', 'wand-sparkles', 'shield-check',
  'archive', 'chat', 'note', 'brain', 'globe', 'settings', 'image', 'translate',
  'research', 'attach', 'doc', 'check', 'edit',
];

/** 完整智能体表单状态(新建与编辑共用)。 */
interface EditorState {
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

function emptyEditor(): EditorState {
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

function toLines(s: string): string[] {
  return s
    .split('\n')
    .map((x) => x.trim())
    .filter(Boolean);
}

export default function AgentPickerDialog({ currentAgentId, onSelect, onClose, sessionId }: Props) {
  const { templates, loaded, load, save, remove } = useAgentsStore();
  const [query, setQuery] = useState('');
  const [applyMsg, setApplyMsg] = useState<string | null>(null);
  const [variant, setVariant] = useState<null | {
    baseId: string;
    name: string;
    model: string;
    strategy: ipc.AgentTemplate['memory_strategy'];
    suffix: string;
  }>(null);
  /** 完整编辑器:新建或编辑自定义模板 */
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!loaded) load();
  }, [loaded, load]);

  /** B4 / F2:仅把渲染后的 prompt 写入会话设置,不改变 Agent 绑定。 */
  const applyPromptOnly = async (agentId: string) => {
    if (!sessionId) return;
    try {
      await ipc.applyAgentPrompt(sessionId, agentId);
      await useSessionsStore.getState().refresh();
      setApplyMsg('已应用为会话系统提示词');
      window.setTimeout(() => setApplyMsg(null), 2200);
    } catch (e) {
      setApplyMsg(`应用失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return templates;
    return templates.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q) ||
        t.category.toLowerCase().includes(q),
    );
  }, [templates, query]);

  const groups = useMemo(() => {
    const map = new Map<string, ipc.AgentTemplate[]>();
    for (const t of filtered) {
      const arr = map.get(t.category) ?? [];
      arr.push(t);
      map.set(t.category, arr);
    }
    return CAT_ORDER.map((k) => ({ cat: k, label: CAT_LABEL[k] ?? k, items: map.get(k) ?? [] })).filter(
      (g) => g.items.length > 0,
    );
  }, [filtered]);

  const onCreateVariant = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!variant) return;
    const base = templates.find((t) => t.agent_id === variant.baseId);
    if (!base) return;
    const id = `${base.agent_id}--${Date.now()}`;
    const name = variant.name.trim() || `${base.name}(变体)`;
    const system_prompt = `${base.system_prompt}${variant.suffix.trim() ? `\n\n${variant.suffix.trim()}` : ''}`;
    await save({
      agent_id: id,
      name,
      icon: base.icon,
      description: base.description,
      category: base.category,
      recommended_model: variant.model.trim() || base.recommended_model,
      memory_strategy: variant.strategy,
      system_prompt,
      input_hints: base.input_hints,
      memory_write: base.memory_write,
      memory_sections: base.memory_sections,
      base_agent: base.agent_id,
      builtin: false,
    });
    setVariant(null);
  };

  const openEdit = (t: ipc.AgentTemplate) => {
    setVariant(null);
    setEditor({
      mode: 'edit',
      agent_id: t.agent_id,
      name: t.name,
      icon: t.icon,
      description: t.description,
      category: t.category,
      recommended_model: t.recommended_model,
      memory_strategy: t.memory_strategy,
      system_prompt: t.system_prompt,
      input_hints: t.input_hints.join('\n'),
      memory_write: t.memory_write,
      memory_sections: t.memory_sections.join('\n'),
      base_agent: t.base_agent ?? null,
    });
  };

  const onSubmitEditor = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editor || saving) return;
    const name = editor.name.trim();
    const prompt = editor.system_prompt.trim();
    if (!name || !prompt) {
      setApplyMsg('名称与 system_prompt 不能为空');
      return;
    }
    setSaving(true);
    try {
      // 新建时用时间戳生成稳定 id(后端禁止与内置 id 冲突)
      const agent_id =
        editor.mode === 'edit'
          ? editor.agent_id
          : `custom--${Date.now().toString(36)}`;
      await save({
        agent_id,
        name,
        icon: editor.icon,
        description: editor.description.trim(),
        category: editor.category,
        recommended_model: editor.recommended_model.trim(),
        memory_strategy: editor.memory_strategy,
        system_prompt: prompt,
        input_hints: toLines(editor.input_hints),
        memory_write: editor.memory_strategy === 'none' ? false : editor.memory_write,
        memory_sections: toLines(editor.memory_sections),
        base_agent: editor.base_agent,
        builtin: false,
      });
      setEditor(null);
      setApplyMsg(editor.mode === 'edit' ? '已保存修改' : '已创建智能体');
      window.setTimeout(() => setApplyMsg(null), 2200);
    } catch (err) {
      setApplyMsg(`保存失败:${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  const Card = ({ t }: { t: ipc.AgentTemplate }) => {
    const Icon = agentIconComponent(t.icon);
    const isActive = currentAgentId === t.agent_id;
    return (
      <div
        className={`agent-card ${isActive ? 'active' : ''}`}
        onClick={() => {
          if (!variant && !editor) onSelect(t.agent_id);
        }}
      >
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
                void applyPromptOnly(t.agent_id);
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
                setEditor(null);
                setVariant({
                  baseId: t.agent_id,
                  name: '',
                  model: '',
                  strategy: t.memory_strategy,
                  suffix: '',
                });
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
                  openEdit(t);
                }}
              >
                编辑
              </button>
              <button
                type="button"
                className="btn ghost micro"
                onClick={(e) => {
                  e.stopPropagation();
                  if (confirm(`删除自定义模板「${t.name}」?`)) remove(t.agent_id);
                }}
              >
                删除
              </button>
            </>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ width: 640, maxHeight: '80vh', display: 'flex', flexDirection: 'column' }}>
        <div className="dialog-head">选择智能体</div>
        <div className="dialog-body" style={{ overflow: 'auto', flex: 1 }}>
          {applyMsg && <div className="toast" style={{ position: 'static', marginBottom: 'var(--space-2)' }}>{applyMsg}</div>}
          <div style={{ display: 'flex', gap: 'var(--space-2)', marginBottom: 'var(--space-3)' }}>
            <input
              className="input"
              placeholder="搜索名称或描述…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              style={{ flex: 1, minWidth: 0 }}
            />
            <button
              type="button"
              className="btn primary"
              onClick={() => {
                setVariant(null);
                setEditor(emptyEditor());
              }}
            >
              新建智能体
            </button>
          </div>

          {/* 空白会话 */}
          <div className="agent-cat">空白会话</div>
          <div className="agent-grid">
            <div
              className={`agent-card ${currentAgentId == null ? 'active' : ''}`}
              onClick={() => onSelect(null)}
            >
              <div className="agent-card-head">
                {(() => {
                  const Icon = agentIconComponent('bot');
                  return <Icon size={18} />;
                })()}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="agent-card-name">空白会话</div>
                  <div className="agent-card-desc">不绑定任何 Agent,使用默认助手行为</div>
                </div>
                {currentAgentId == null && <span className="agent-card-badge" style={{ background: 'var(--ok)' }}>当前</span>}
              </div>
            </div>
          </div>

          {groups.map((g) => (
            <div key={g.cat}>
              <div className="agent-cat">{g.label}</div>
              <div className="agent-grid">
                {g.items.map((t) => (
                  <Card key={t.agent_id} t={t} />
                ))}
              </div>
            </div>
          ))}

          {filtered.length === 0 && (
            <div style={{ textAlign: 'center', color: 'var(--text-4)', padding: 'var(--space-6)' }}>
              未找到匹配的智能体
            </div>
          )}

          {/* 完整智能体编辑器(新建 / 编辑自定义模板) */}
          {editor && (
            <div style={{ marginTop: 'var(--space-4)', borderTop: '1px solid var(--border)', paddingTop: 'var(--space-4)' }}>
              <div style={{ fontWeight: 700, marginBottom: 'var(--space-2)' }}>
                {editor.mode === 'edit' ? '编辑智能体' : '新建智能体'}
              </div>
              <form onSubmit={onSubmitEditor} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
                <div className="agent-form-row">
                  <div>
                    <label className="lbl">名称 *</label>
                    <input
                      className="input"
                      value={editor.name}
                      onChange={(e) => setEditor({ ...editor, name: e.target.value })}
                      placeholder="如:简历优化助手"
                    />
                  </div>
                  <div>
                    <label className="lbl">分类</label>
                    <select
                      className="input"
                      value={editor.category}
                      onChange={(e) => setEditor({ ...editor, category: e.target.value })}
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
                      onChange={(e) => setEditor({ ...editor, icon: e.target.value })}
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
                      onChange={(e) => setEditor({ ...editor, recommended_model: e.target.value })}
                      placeholder="如 gpt-5.2"
                    />
                  </div>
                </div>
                <div>
                  <label className="lbl">描述</label>
                  <input
                    className="input"
                    value={editor.description}
                    onChange={(e) => setEditor({ ...editor, description: e.target.value })}
                    placeholder="一句话说明这个智能体做什么"
                  />
                </div>
                <div>
                  <label className="lbl">记忆注入策略</label>
                  <select
                    className="input"
                    value={editor.memory_strategy}
                    onChange={(e) =>
                      setEditor({ ...editor, memory_strategy: e.target.value as ipc.AgentTemplate['memory_strategy'] })
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
                    onChange={(e) => setEditor({ ...editor, system_prompt: e.target.value })}
                    placeholder="角色 / 能力边界 / 输出格式 / 约束…(支持 {date} {model} {language} 变量)"
                  />
                </div>
                <div>
                  <label className="lbl">输入提示(一行一条)</label>
                  <textarea
                    className="input"
                    rows={3}
                    value={editor.input_hints}
                    onChange={(e) => setEditor({ ...editor, input_hints: e.target.value })}
                    placeholder="帮我优化这段文字"
                  />
                </div>
                <label className="agent-check">
                  <input
                    type="checkbox"
                    checked={editor.memory_write}
                    disabled={editor.memory_strategy === 'none'}
                    onChange={(e) => setEditor({ ...editor, memory_write: e.target.checked })}
                  />
                  参与记忆自动提炼(strategy=none 时不可选)
                </label>
                <div>
                  <label className="lbl">写入记忆的节(一行一节)</label>
                  <textarea
                    className="input"
                    rows={3}
                    value={editor.memory_sections}
                    onChange={(e) => setEditor({ ...editor, memory_sections: e.target.value })}
                    placeholder="会话摘要"
                  />
                </div>
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
                  <button type="button" className="btn ghost" onClick={() => setEditor(null)}>取消</button>
                  <button type="submit" className="btn primary" disabled={saving}>
                    {saving ? '保存中…' : '保存'}
                  </button>
                </div>
              </form>
            </div>
          )}

          {/* 变体创建表单 */}
          {variant && (
            <div style={{ marginTop: 'var(--space-4)', borderTop: '1px solid var(--border)', paddingTop: 'var(--space-4)' }}>
              <div style={{ fontWeight: 700, marginBottom: 'var(--space-2)' }}>创建变体</div>
              <form onSubmit={onCreateVariant} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
                <div>
                  <label className="lbl">名称</label>
                  <input className="input" value={variant.name} onChange={(e) => setVariant({ ...variant, name: e.target.value })} placeholder="如:产品助手-深度版" />
                </div>
                <div>
                  <label className="lbl">推荐模型</label>
                  <input className="input" value={variant.model} onChange={(e) => setVariant({ ...variant, model: e.target.value })} placeholder={`缺省 = ${templates.find((t) => t.agent_id === variant.baseId)?.recommended_model ?? ''}`} />
                </div>
                <div>
                  <label className="lbl">记忆注入策略</label>
                  <select
                    className="input"
                    value={variant.strategy}
                    onChange={(e) => setVariant({ ...variant, strategy: e.target.value as ipc.AgentTemplate['memory_strategy'] })}
                  >
                    <option value="full">full</option>
                    <option value="summary">summary</option>
                    <option value="none">none</option>
                  </select>
                </div>
                <div>
                  <label className="lbl">system_prompt 追加后缀</label>
                  <textarea className="input" rows={3} value={variant.suffix} onChange={(e) => setVariant({ ...variant, suffix: e.target.value })} placeholder="追加到原 system prompt 末尾的约束/风格…" />
                </div>
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
                  <button type="button" className="btn ghost" onClick={() => setVariant(null)}>取消</button>
                  <button type="submit" className="btn primary">保存变体</button>
                </div>
              </form>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
