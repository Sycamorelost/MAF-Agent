import { useEffect, useState } from 'react';
import { StagePipelineEditor } from '../../components/stage/StagePipelineEditor';
import * as ipc from '../../lib/ipc';
import { IconCheck, IconEdit } from '../../components/icons';

export function AdjustStagesDialog({
  researchId,
  stage,
  onClose,
  onSaved,
}: {
  researchId: string;
  /** 已完成的阶段序号(1-based),仅其后阶段可编辑 */
  stage: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [configs, setConfigs] = useState<ipc.StageConfig[] | null>(null);
  const [edited, setEdited] = useState<ipc.StageConfig[]>([]);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveErr, setSaveErr] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    ipc
      .researchGet(researchId)
      .then((d) => {
        if (!live) return;
        const cfgs = d.stages?.configs ?? [];
        setConfigs(cfgs);
        setEdited(cfgs.slice(stage));
      })
      .catch((e) => live && setLoadErr(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [researchId, stage]);

  const onConfirm = async () => {
    if (!configs || saving) return;
    setSaving(true);
    setSaveErr(null);
    try {
      // 合并:前 stage 个已完成阶段保持原样 + 编辑后的剩余阶段
      await ipc.researchUpdateStages(researchId, [...configs.slice(0, stage), ...edited]);
      onSaved();
    } catch (e) {
      setSaveErr(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  const editable = configs !== null && configs.length > 0;
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" style={{ width: 640, maxWidth: '92vw' }} onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <IconEdit size={16} />
          调整后续阶段
        </div>
        <div className="dialog-body">
          <div className="pg-hint" style={{ marginBottom: 8 }}>
            仅编辑阶段 {stage} 之后的剩余阶段;已完成阶段不受影响。保存后立即继续执行。
          </div>
          {loadErr && <div className="pg-hint">加载失败:{loadErr}</div>}
          {!loadErr && configs === null && <div className="pg-hint">加载阶段配置中…</div>}
          {!loadErr && configs !== null && configs.length === 0 && (
            <div className="pg-hint">该研究没有阶段配置快照(旧数据),无法调整。</div>
          )}
          {editable && <StagePipelineEditor stages={edited} onChange={setEdited} />}
        </div>
        <div className="tpl-foot">
          {saveErr && <span className="tpl-foot-err">{saveErr}</span>}
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button
            className="btn-primary"
            disabled={saving || !editable}
            onClick={() => void onConfirm()}
          >
            <IconCheck size={13} />
            {saving ? '保存中…' : '保存并继续'}
          </button>
        </div>
      </div>
    </div>
  );
}

/** 检查点操作条:阶段间暂停,确认后进入下一阶段。 */



