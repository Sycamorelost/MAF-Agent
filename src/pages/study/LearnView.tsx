import { useCallback, useEffect, useState } from 'react';
import { MemoMarkdown } from '../../components/MemoMarkdown';
import { IconArrowLeft } from '../../components/icons';
import { resolveProvider } from '../../stores/study';
import type { StudyUnit } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';

/* ---------------- 学习视图(自学材料) ---------------- */

export function LearnView({
  planId,
  nodeId,
  nodeName,
  onClose,
  onChanged,
  onQuiz,
}: {
  planId: string;
  nodeId: number;
  nodeName: string;
  onClose: () => void;
  onChanged: () => void;
  onQuiz: () => void;
}) {
  const [unit, setUnit] = useState<StudyUnit | null>(null);
  const [busy, setBusy] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [freeForm, setFreeForm] = useState(false);
  const [rated, setRated] = useState(false);

  const load = useCallback(
    async (force: boolean) => {
      setBusy(true);
      setErr(null);
      try {
        const prov = resolveProvider();
        const u = await studyIpc.studyUnitGenerate({
          id: planId,
          node_id: nodeId,
          free_form: freeForm,
          force,
          ...prov,
        });
        setUnit(u);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [planId, nodeId, freeForm],
  );

  useEffect(() => {
    void load(false);
  }, [load]);

  const rate = async (rating: number) => {
    try {
      await studyIpc.studyMasterySet(planId, nodeId, rating);
      setRated(true);
      onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  };

  const markDone = async () => {
    try {
      await studyIpc.studyNodeDone(planId, nodeId, true);
      onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="rp-timeline">
      <div className="pg-sec st-flex">
        <button className="btn-ghost" onClick={onClose}>
          <IconArrowLeft size={14} />
          返回
        </button>
        <b>{nodeName}</b>
        <span className="pg-chip">自学材料{unit ? ` · 约 ${unit.est_minutes} 分钟` : ''}</span>
        <span className="sp" />
        <button
          className={`pg-chip${freeForm ? ' on' : ''}`}
          title="重新生成时不套固定小节结构,按内容自然组织"
          onClick={() => setFreeForm((v) => !v)}
        >
          自由结构
        </button>
        <button className="pg-chip" disabled={busy} onClick={() => void load(true)}>
          重新生成
        </button>
        <button className="pg-chip" onClick={onQuiz}>
          测验
        </button>
      </div>
      {err && <div className="pg-warn">{err}</div>}
      {busy && <div className="pg-hint">正在生成自学材料,约需十几秒…</div>}
      {!busy && unit && (
        <div className="md report-md st-mt-2">
          <MemoMarkdown content={unit.content_md} />
        </div>
      )}
      {unit && (
        <div className="pg-sec st-mt-3">
          <div className="pg-lab">读完自评掌握</div>
          <div className="pg-chips">
            <button className="pg-chip" title="还没理解,稍后再来" onClick={() => void rate(0)}>
              没懂
            </button>
            <button className="pg-chip" title="大致理解了" onClick={() => void rate(0.5)}>
              大概懂
            </button>
            <button className="pg-chip" title="已掌握,标记知识点完成" onClick={() => void rate(1)}>
              掌握
            </button>
            <button className="pg-chip" onClick={() => void markDone()}>
              标记完成
            </button>
          </div>
          {rated && (
            <div className="pg-hint">
              已记录。掌握度 = 自评 ×0.4 + 测验 ×0.6,做一次测验更能校准;「掌握」同时把知识点标记为完成。
            </div>
          )}
        </div>
      )}
      {unit && (
        <div className="pg-sec st-mt-3 st-flex">
          <button className="btn-primary" onClick={onQuiz}>
            我学完了,立即小测
          </button>
          <span className="pg-hint">5 道题校准掌握度,客观题即时判分</span>
        </div>
      )}
    </div>
  );
}
