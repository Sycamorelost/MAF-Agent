import { useState } from 'react';
import { useResearchStore } from '../../stores/research';
import { AdjustStagesDialog } from './AdjustStagesDialog';
import { IconEdit, IconPause, IconPlay } from '../../components/icons';

export function CheckpointBar({ stage, next }: { stage: number; next: string }) {
  const stageContinue = useResearchStore((s) => s.stageContinue);
  const openDetail = useResearchStore((s) => s.openDetail);
  const activeId = useResearchStore((s) => s.active?.id);
  const [adjusting, setAdjusting] = useState(false);
  return (
    <div className="checkpoint-bar">
      <span className="cp-ic">
        <IconPause size={16} />
      </span>
      <div className="cp-t">
        <div className="t1">
          检查点:阶段 {stage} 已完成,等待确认进入「{next}」
        </div>
        <div className="t2">可先查看上方阶段摘要,或调整下阶段参数与提示词后再继续</div>
      </div>
      <button
        className="btn-ghost"
        style={{ border: '1px solid var(--border-strong)' }}
        onClick={() => setAdjusting(true)}
      >
        <IconEdit size={13} />
        调整下阶段
      </button>
      <button className="btn-primary" onClick={stageContinue}>
        <IconPlay size={13} />
        继续阶段 {stage + 1}
      </button>
      {adjusting && activeId && (
        <AdjustStagesDialog
          researchId={activeId}
          stage={stage}
          onClose={() => setAdjusting(false)}
          onSaved={() => {
            setAdjusting(false);
            // 刷新阶段快照(新增/停用立即反映到流水线展示),无论成败都继续执行
            void openDetail(activeId).finally(() => stageContinue());
          }}
        />
      )}
    </div>
  );
}



