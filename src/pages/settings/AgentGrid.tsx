import * as ipc from '../../lib/ipc';
import { agentIconComponent } from '../../components/icons';
import { uiConfirm } from '../../components/confirmService';

/** 按分类分组后的智能体列表(用于网格渲染)。 */
export interface AgentGroup {
  cat: string;
  label: string;
  items: ipc.AgentTemplate[];
}

interface AgentGridProps {
  groups: AgentGroup[];
  /** 导出单个智能体到文件(提示信息由调用方负责)。 */
  onExport: (t: ipc.AgentTemplate) => Promise<void>;
  onEdit: (t: ipc.AgentTemplate) => void;
  onCreateVariant: (t: ipc.AgentTemplate) => void;
  /** 已确认后执行删除 / 恢复内置。 */
  onRemove: (t: ipc.AgentTemplate) => void;
}

/** 智能体卡片网格:按分类分组展示,含导出 / 编辑 / 恢复 / 删除 / 创建变体操作。 */
export function AgentGrid({ groups, onExport, onEdit, onCreateVariant, onRemove }: AgentGridProps) {
  if (groups.length === 0) {
    return (
      <div style={{ textAlign: 'center', color: 'var(--text-4)', padding: 'var(--space-6)' }}>
        未找到匹配的智能体
      </div>
    );
  }

  return (
    <>
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
                      onClick={() => {
                        void onExport(t);
                      }}
                    >
                      导出
                    </button>
                    {t.builtin ? (
                      <>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => onEdit(t)}
                        >
                          编辑
                        </button>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => onCreateVariant(t)}
                        >
                          创建变体
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => onEdit(t)}
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
                                onRemove(t);
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
                                onRemove(t);
                            }}
                          >
                            删除
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn ghost micro"
                          onClick={() => onCreateVariant(t)}
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
    </>
  );
}
