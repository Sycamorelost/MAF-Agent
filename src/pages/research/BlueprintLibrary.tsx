import { useEffect, useState } from 'react';
import { useResearchStore } from '../../stores/research';
import * as ipc from '../../lib/ipc';
import { blueprintMeta, type BpDlgState } from './helpers';
import { IconArrowLeft, IconCheck, IconCopy, IconEdit, IconGlobe, IconPlus, IconResearch, IconTrash } from '../../components/icons';

export function BlueprintLibrary({
  onBack,
  onApplied,
  onSaveCurrent,
}: {
  onBack: () => void;
  onApplied: () => void;
  onSaveCurrent: () => void;
}) {
  const { blueprints, loadBlueprints, saveBlueprint, removeBlueprint, applyBlueprint } =
    useResearchStore();
  const [dlg, setDlg] = useState<BpDlgState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadBlueprints().catch(() => {});
  }, [loadBlueprints]);

  const onApply = (bp: ipc.Blueprint) => {
    applyBlueprint(bp);
    onApplied();
  };

  const onConfirmDlg = async () => {
    if (!dlg) return;
    const name = dlg.name.trim();
    if (!name) return;
    setError(null);
    try {
      if (dlg.kind === 'edit' && dlg.src) {
        // 改名 = 另存新名 + 删除旧条目
        await saveBlueprint(name, dlg.desc.trim(), dlg.src.params);
        if (name !== dlg.src.name) await removeBlueprint(dlg.src.name);
      } else if (dlg.kind === 'saveas' && dlg.src) {
        await saveBlueprint(name, dlg.desc.trim(), dlg.src.params);
      } else {
        await saveBlueprint(name, dlg.desc.trim(), { ...ipc.DEFAULT_RESEARCH_PARAMS });
      }
      setDlg(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="rp-lib">
      <div className="pg-main-head">
        <button onClick={onBack} title="返回研究历史">
          <IconArrowLeft size={14} />
          返回
        </button>
        <span className="topic">方案库</span>
        <span className="cost">参数 + 阶段编排 + 提示词 + 约束,一体保存复用</span>
        <button
          className="btn-primary"
          style={{ border: 'none' }}
          onClick={() => setDlg({ kind: 'new', src: null, name: '', desc: '' })}
        >
          <IconPlus size={14} />
          新建方案
        </button>
      </div>
      <div className="rp-lib-body">
        <div className="bp-grid">
          {blueprints.map((bp) => (
            <div key={bp.name} className="bp-card">
              <div className="bp-h">
                <span className={`ic${bp.builtin ? '' : ' user'}`}>
                  {bp.builtin ? <IconResearch size={17} /> : <IconGlobe size={17} />}
                </span>
                <span className="nm ellipsis" title={bp.name}>
                  {bp.name}
                </span>
                <span className={`tag${bp.builtin ? ' builtin' : ''}`}>
                  {bp.builtin ? '内置' : '我的方案'}
                </span>
              </div>
              <div className="bp-d">{bp.desc || '暂无描述'}</div>
              <div className="bp-meta">
                {blueprintMeta(bp).map((m) => (
                  <span key={m} className="ov-chip set">
                    {m}
                  </span>
                ))}
                {/* L2:方案不含执行模型,固定提示跟随启动时选择 */}
                <span className="ov-chip">执行模型:跟随当前</span>
              </div>
              <div className="bp-ft">
                <button
                  className="btn-brand"
                  style={{ flex: 1, justifyContent: 'center' }}
                  onClick={() => onApply(bp)}
                >
                  应用
                </button>
                <button
                  className="btn-ghost"
                  style={{ border: '1px solid var(--border)' }}
                  title="以此方案为基础另存为新方案"
                  onClick={() => setDlg({ kind: 'saveas', src: bp, name: `${bp.name} 副本`, desc: bp.desc })}
                >
                  <IconCopy size={13} />
                  另存为
                </button>
                {!bp.builtin && (
                  <>
                    <button
                      className="btn-ghost"
                      style={{ border: '1px solid var(--border)' }}
                      title="编辑名称与描述"
                      onClick={() => setDlg({ kind: 'edit', src: bp, name: bp.name, desc: bp.desc })}
                    >
                      <IconEdit size={13} />
                    </button>
                    <button
                      className="btn-ghost"
                      style={{ border: '1px solid var(--border)', color: 'var(--err)' }}
                      title="删除方案"
                      onClick={() => {
                        if (confirm(`删除方案「${bp.name}」?`)) void removeBlueprint(bp.name);
                      }}
                    >
                      <IconTrash size={13} />
                    </button>
                  </>
                )}
              </div>
            </div>
          ))}
          <div
            className="bp-card bp-new"
            onClick={onSaveCurrent}
            title="回到配置页,将当前配置保存为方案"
          >
            <IconPlus size={22} />
            <span>从当前配置另存为新方案</span>
          </div>
        </div>
        <div className="pg-hint" style={{ padding: '0 var(--pad-x) var(--space-4)' }}>
          旧版「研究预设」已自动迁移为方案(仅参数、快速模式),迁移备份保留于
          data/research/presets.json.migrated。
        </div>
      </div>

      {dlg && (
        <div className="dialog-mask" onClick={() => setDlg(null)}>
          <div className="dialog" style={{ width: 420 }} onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              {dlg.kind === 'edit' ? <IconEdit size={16} /> : <IconPlus size={16} />}
              {dlg.kind === 'edit' ? '编辑方案' : dlg.kind === 'saveas' ? '另存为方案' : '新建方案'}
            </div>
            <div className="dialog-body" style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
              <div className="ov-row">
                <span className="lbl">方案名称</span>
                <input
                  className="pg-field"
                  style={{ width: '100%' }}
                  placeholder="如:竞品周报"
                  value={dlg.name}
                  autoFocus
                  onChange={(e) => setDlg({ ...dlg, name: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void onConfirmDlg();
                    if (e.key === 'Escape') setDlg(null);
                  }}
                />
              </div>
              <div className="ov-row">
                <span className="lbl">描述</span>
                <textarea
                  className="pg-field"
                  rows={3}
                  style={{ width: '100%' }}
                  placeholder="阶段构成、适用场景、约束要点…"
                  value={dlg.desc}
                  onChange={(e) => setDlg({ ...dlg, desc: e.target.value })}
                />
              </div>
              {error && <div className="pg-hint" style={{ color: 'var(--err)' }}>{error}</div>}
              {dlg.kind !== 'edit' && (
                <div className="pg-hint">
                  {dlg.kind === 'saveas'
                    ? '参数、阶段编排、提示词与约束将完整复制;名称不可与内置方案重复。'
                    : '先以默认参数创建;应用后在配置页调整,再「存为方案」覆盖。'}
                </div>
              )}
            </div>
            <div className="tpl-foot">
              <span className="sp" />
              <button className="btn-ghost" onClick={() => setDlg(null)}>
                取消
              </button>
              <button className="btn-primary" onClick={() => void onConfirmDlg()} disabled={!dlg.name.trim()}>
                <IconCheck size={13} />
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}


