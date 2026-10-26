import { useEffect, useMemo, useState } from 'react';
import { useAgentsStore } from '../../stores/agents';
import { useSessionsStore } from '../../stores/sessions';
import { agentIconComponent } from '../icons';
import * as ipc from '../../lib/ipc';
import {
  AgentCard,
  AgentEditorForm,
  emptyEditor,
  toLines,
  CAT_LABEL,
  CAT_ORDER,
  type EditorState,
} from './AgentCard';
interface Props {
  /** 当前会话已绑定的 agent_id(null = 空白) */
  currentAgentId?: string | null;
  /** 选中后回调;id=null 表示空白会话 */
  onSelect: (agentId: string | null) => void;
  onClose: () => void;
  /** B4 / F2:存在时卡片显示「应用为提示词」(仅写会话 system_prompt,不绑定 Agent) */
  sessionId?: string | null;
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
  /** Esc 关闭(与全局搜索一致,避免只能点遮罩) */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
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
  /** 编辑 / 变体表单打开时,卡片点击不改绑定(与原行为一致)。 */
  const selectCard = (agentId: string) => {
    if (!variant && !editor) onSelect(agentId);
  };

  const openVariant = (t: ipc.AgentTemplate) => {
    setEditor(null);
    setVariant({
      baseId: t.agent_id,
      name: '',
      model: '',
      strategy: t.memory_strategy,
      suffix: '',
    });
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
                  <AgentCard
                    key={t.agent_id}
                    t={t}
                    isActive={currentAgentId === t.agent_id}
                    sessionId={sessionId}
                    onSelect={selectCard}
                    onApplyPrompt={applyPromptOnly}
                    onCreateVariant={openVariant}
                    onEdit={openEdit}
                    onRemove={remove}
                  />
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
            <AgentEditorForm
              editor={editor}
              saving={saving}
              onChange={setEditor}
              onCancel={() => setEditor(null)}
              onSubmit={onSubmitEditor}
            />
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
