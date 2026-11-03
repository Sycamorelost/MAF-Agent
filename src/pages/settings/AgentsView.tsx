import { useEffect, useMemo, useRef, useState } from 'react';
import { useAgentsStore } from '../../stores/agents';
import { useProvidersStore } from '../../stores/providers';
import * as ipc from '../../lib/ipc';
import { agentIconComponent } from '../../components/icons';
import { uiConfirm } from '../../components/confirmService';
import {
  AgentEditorForm,
  emptyEditor,
  toLines,
  CAT_LABEL,
  CAT_ORDER,
  type EditorState,
} from '../../components/agent/AgentCard';

/** 设置页 → 智能体管理:列表 / 新建 / 编辑 / 删除 / 创建变体。 */
export function AgentsView() {
  const { templates, loaded, load, save, remove } = useAgentsStore();
  const { templates: providers } = useProvidersStore();

  const [query, setQuery] = useState('');
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  /** 简单/高级模式(默认简单;编辑已存在智能体时跟随其复杂度)。 */
  const [advanced, setAdvanced] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [variant, setVariant] = useState<null | {
    baseId: string;
    name: string;
    model: string;
    strategy: ipc.AgentTemplate['memory_strategy'];
    suffix: string;
  }>(null);

  useEffect(() => {
    if (!loaded) load();
  }, [loaded, load]);

  /** 可用模型列表(所有服务商已配置的模型并集)。 */
  const availableModels = useMemo(() => {
    const set = new Set<string>();
    for (const p of providers) for (const m of p.models) set.add(m);
    return Array.from(set);
  }, [providers]);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = q
      ? templates.filter(
          (t) =>
            t.name.toLowerCase().includes(q) ||
            t.description.toLowerCase().includes(q) ||
            t.category.toLowerCase().includes(q),
        )
      : templates;
    const map = new Map<string, ipc.AgentTemplate[]>();
    for (const t of list) {
      const arr = map.get(t.category) ?? [];
      arr.push(t);
      map.set(t.category, arr);
    }
    return CAT_ORDER.map((k) => ({ cat: k, label: CAT_LABEL[k] ?? k, items: map.get(k) ?? [] })).filter(
      (g) => g.items.length > 0,
    );
  }, [templates, query]);

  const openNew = () => {
    setVariant(null);
    setAdvanced(false);
    setEditor(emptyEditor());
  };

  const openEdit = (t: ipc.AgentTemplate) => {
    setVariant(null);
    // 若模板含 v2 高级字段(开场白/建议/工具策略等非空),默认进入高级模式
    const hasAdvanced =
      (t.opening_message ?? '').length > 0 ||
      (t.suggestions ?? []).length > 0 ||
      t.tool_policy != null ||
      (t.thinking_level ?? '').length > 0 ||
      t.temperature != null ||
      (t.doc_pipeline ?? '').length > 0 ||
      (t.triggers ?? []).length > 0 ||
      t.preset != null;
    setAdvanced(hasAdvanced);
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
      input_hints: (t.input_hints ?? []).join('\n'),
      memory_write: t.memory_write,
      memory_sections: (t.memory_sections ?? []).join('\n'),
      base_agent: t.base_agent ?? null,
      storage: t.storage === 'decoupled' ? 'decoupled' : 'coupled',
      prompt_file: t.prompt_file ?? '',
      tools_file: t.tools_file ?? '',
    });
  };

  const openVariant = (t: ipc.AgentTemplate) => {
    setEditor(null);
    setVariant({
      baseId: t.agent_id,
      name: '',
      model: t.recommended_model,
      strategy: t.memory_strategy,
      suffix: '',
    });
  };

  const onSubmitEditor = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editor || saving) return;
    const name = editor.name.trim();
    const prompt = editor.system_prompt.trim();
    if (!name || !prompt) {
      setMsg('名称与 system_prompt 不能为空');
      return;
    }
    setSaving(true);
    try {
      const agent_id =
        editor.mode === 'edit' ? editor.agent_id : `custom--${Date.now().toString(36)}`;
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
        storage: editor.storage,
        prompt_file: editor.storage === 'decoupled' ? editor.prompt_file || null : null,
        tools_file: editor.storage === 'decoupled' ? editor.tools_file || null : null,
      });
      setEditor(null);
      setMsg(editor.mode === 'edit' ? '已保存修改' : '已创建智能体');
      window.setTimeout(() => setMsg(null), 2200);
    } catch (err) {
      setMsg(`保存失败:${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  const onCreateVariant = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!variant) return;
    const base = templates.find((t) => t.agent_id === variant.baseId);
    if (!base) return;
    setSaving(true);
    try {
      const id = `${base.agent_id}--${Date.now().toString(36)}`;
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
      setMsg('已创建变体');
      window.setTimeout(() => setMsg(null), 2200);
    } catch (err) {
      setMsg(`保存失败:${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="set-block">
      <div className="set-head">智能体管理</div>
      <p className="set-desc">
        管理全部智能体(内置 + 自定义)。内置智能体不可删除,可「创建变体」另存为自定义;
        新建可选简单模式(核心字段)或高级模式(完整配置)。
      </p>

      {msg && (
        <div
          style={{
            padding: 'var(--space-2) var(--space-3)',
            borderRadius: 'var(--radius-s)',
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border)',
            color: 'var(--text-1)',
            fontSize: 'var(--font-size-sm)',
            marginBottom: 'var(--space-2)',
          }}
        >
          {msg}
        </div>
      )}

      <div style={{ display: 'flex', gap: 'var(--space-2)', marginBottom: 'var(--space-3)' }}>
        <input
          className="input"
          placeholder="搜索名称或描述…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{ flex: 1, minWidth: 0 }}
        />
        <button type="button" className="btn primary" onClick={openNew}>
          新建智能体
        </button>
        <button
          type="button"
          className="btn ghost"
          onClick={() => fileInputRef.current?.click()}
        >
          导入
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept="application/json,.json"
          style={{ display: 'none' }}
          onChange={async (e) => {
            const f = e.target.files?.[0];
            e.target.value = '';
            if (!f) return;
            try {
              const text = await f.text();
              await ipc.importAgentTemplate(text);
              setMsg('导入成功');
              window.setTimeout(() => setMsg(null), 2000);
              load();
            } catch (err) {
              setMsg(`导入失败:${err instanceof Error ? err.message : String(err)}`);
            }
          }}
        />
      </div>

      {groups.map((g) => (
        <div key={g.cat} style={{ marginBottom: 'var(--space-4)' }}>
          <div className="agent-cat">{g.label}</div>
          <div className="agent-grid">
            {g.items.map((t) => {
              const Icon = agentIconComponent(t.icon);
              return (
                <div key={t.agent_id} className="agent-card">
                  <div className="agent-card-head">
                    <Icon size={18} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div className="agent-card-name">{t.name}</div>
                      <div className="agent-card-desc">{t.description}</div>
                    </div>
                    {t.builtin ? (
                      <span className="agent-card-badge">内置</span>
                    ) : t.overrides_builtin ? (
                      <span className="agent-card-badge">已覆盖</span>
                    ) : t.base_agent ? (
                      <span className="agent-card-badge">变体</span>
                    ) : null}
                  </div>
                  <div className="agent-card-meta">
                    <span>模型 {t.recommended_model || '跟随全局'}</span>
                    <span>·</span>
                    <span>
                      {t.memory_strategy === 'none' ? '不注入记忆' : `注入 ${t.memory_strategy}`}
                    </span>
                  </div>
                  <div className="agent-card-actions">
                    <button
                      type="button"
                      className="btn ghost micro"
                      onClick={async () => {
                        try {
                          const path = await ipc.exportAgentToFile(t.agent_id);
                          if (path) {
                            setMsg(`已导出「${t.name}」到:${path}`);
                            window.setTimeout(() => setMsg(null), 2600);
                          }
                          // 用户取消(path 为 null)不提示
                        } catch (err) {
                          setMsg(`导出失败:${err instanceof Error ? err.message : String(err)}`);
                          window.setTimeout(() => setMsg(null), 2600);
                        }
                      }}
                    >
                      导出
                    </button>
                    {t.builtin ? (
                      <>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => openEdit(t)}
                        >
                          编辑
                        </button>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => openVariant(t)}
                        >
                          创建变体
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => openEdit(t)}
                        >
                          编辑
                        </button>
                        {t.overrides_builtin && (
                          <button
                            type="button"
                            className="btn ghost micro"
                            onClick={async () => {
                              if (
                                await uiConfirm({
                                  title: '恢复内置',
                                  message: `恢复内置智能体「${t.name}」的原始配置?`,
                                  confirmText: '恢复',
                                })
                              )
                                remove(t.agent_id);
                            }}
                          >
                            恢复内置
                          </button>
                        )}
                        {!t.overrides_builtin && (
                          <button
                            type="button"
                            className="btn ghost micro"
                            onClick={async () => {
                              if (
                                await uiConfirm({
                                  title: '删除智能体',
                                  message: `删除自定义智能体「${t.name}」?`,
                                  danger: true,
                                  confirmText: '删除',
                                })
                              )
                                remove(t.agent_id);
                            }}
                          >
                            删除
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => openVariant(t)}
                        >
                          创建变体
                        </button>
                      </>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ))}

      {groups.length === 0 && (
        <div style={{ textAlign: 'center', color: 'var(--text-4)', padding: 'var(--space-6)' }}>
          未找到匹配的智能体
        </div>
      )}

      {/* 编辑器 */}
      {editor && (
        <div className="dialog-mask" onClick={() => setEditor(null)}>
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
                  onChange={(e) => setAdvanced(e.target.checked)}
                />
                高级模式(展示输入提示、记忆提炼等高级字段)
              </label>
              <AgentEditorForm
                editor={editor}
                saving={saving}
                onChange={setEditor}
                onCancel={() => setEditor(null)}
                onSubmit={onSubmitEditor}
                advanced={advanced}
                availableModels={availableModels}
              />
            </div>
          </div>
        </div>
      )}

      {/* 变体创建表单 */}
      {variant && (
        <div className="dialog-mask" onClick={() => setVariant(null)}>
          <div
            className="dialog"
            onClick={(e) => e.stopPropagation()}
            style={{ width: 'min(calc(520px * var(--ui-scale)), 92vw)' }}
          >
            <div className="dialog-head">创建变体</div>
            <div className="dialog-body">
              <form onSubmit={onCreateVariant} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
                <div>
                  <label className="lbl">名称</label>
                  <input
                    className="input"
                    value={variant.name}
                    onChange={(e) => setVariant({ ...variant, name: e.target.value })}
                    placeholder="如:产品助手-深度版"
                  />
                </div>
                <div>
                  <label className="lbl">推荐模型</label>
                  {availableModels.length > 0 ? (
                    <select
                      className="input"
                      value={variant.model}
                      onChange={(e) => setVariant({ ...variant, model: e.target.value })}
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
                      onChange={(e) => setVariant({ ...variant, model: e.target.value })}
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
                      setVariant({ ...variant, strategy: e.target.value as ipc.AgentTemplate['memory_strategy'] })
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
                    onChange={(e) => setVariant({ ...variant, suffix: e.target.value })}
                    placeholder="追加到原 system prompt 末尾的约束/风格…"
                  />
                </div>
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
                  <button type="button" className="btn ghost" onClick={() => setVariant(null)}>
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
      )}
    </div>
  );
}
