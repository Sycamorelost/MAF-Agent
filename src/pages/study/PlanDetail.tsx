import { useMemo, useState } from 'react';
import { IconCheck, IconDownload, IconResearch } from '../../components/icons';
import type { NodeState, PlanPayload, StudyStats, TodayPlan } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';
import { EXPORT_TARGET_EXISTS, pickSavePath } from '../../lib/ipc/research';
import { safeExportName } from '../research/helpers';
import { levelText } from './PlanList';
import { StatsSec } from './StatsSec';

/* ---------------- 导出按钮 ---------------- */

export function ExportBtn({ id, topic }: { id: string; topic: string }) {
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  return (
    <span style={{ display: 'inline-flex', gap: 8, alignItems: 'center' }}>
      {hint && <span className="pg-hint ellipsis">{hint}</span>}
      <button
        className="btn-ghost"
        disabled={busy}
        onClick={async () => {
          if (busy) return;
          setBusy(true);
          setHint(null);
          try {
            const pick = await pickSavePath('导出学习计划(Markdown)', `${safeExportName(topic)}.md`, 'Markdown 文档', 'md');
            if (!pick) return;
            const doExport = (overwrite: boolean) => studyIpc.studyExportMarkdown(id, pick.token, overwrite);
            let saved: string;
            try {
              saved = await doExport(pick.existed);
            } catch (e) {
              const msg = e instanceof Error ? e.message : String(e);
              if (msg === EXPORT_TARGET_EXISTS) {
                if (!window.confirm(`目标文件已存在:\n${pick.path}\n\n确认覆盖?`)) return;
                saved = await doExport(true);
              } else {
                throw e;
              }
            }
            setHint(saved ? `已导出:${saved}` : null);
          } catch (e) {
            setHint(`导出失败:${e instanceof Error ? e.message : String(e)}`);
          } finally {
            setBusy(false);
          }
        }}
      >
        <IconDownload size={14} />
        {busy ? '导出中…' : '导出'}
      </button>
    </span>
  );
}

/* ---------------- 计划总览(今日待办 + 阶段树 + 资料 + 排期 + 统计) ---------------- */

export function PlanDetail({
  payload,
  states,
  today,
  stats,
  onLearn,
  onQuiz,
  onDeepDig,
  onReview,
  onChanged,
}: {
  payload: PlanPayload;
  states: NodeState[];
  today: TodayPlan | null;
  stats: StudyStats | null;
  onLearn: (nodeId: number, name: string) => void;
  onQuiz: (nodeId: number, name: string) => void;
  onDeepDig: (node: string) => void;
  onReview: (nodeId: number, correct: boolean) => void;
  onChanged: () => void;
}) {
  const stateMap = useMemo(() => new Map(states.map((s) => [s.name, s])), [states]);
  return (
    <div className="rp-timeline">
      <div className="pg-sec">
        <div className="pg-lab">计划总览</div>
        <div className="pg-chips">
          <span className="pg-chip on">{levelText(payload.level)}</span>
          <span className="pg-chip">每天 {payload.daily_minutes} 分钟</span>
          {payload.deadline && <span className="pg-chip">截止 {payload.deadline}</span>}
          <span className="pg-chip">共 {payload.schedule.length} 周</span>
          <span className="pg-chip">{payload.nodes.length} 个知识点</span>
        </div>
        {payload.goal && <div className="pg-hint">学习目标:{payload.goal}</div>}
        {payload.repaired && (
          <div className="pg-warn">本计划为校验未通过后的降级结构,建议人工复核阶段划分。</div>
        )}
      </div>

      {/* 今日待办(P1):当前排期周应学知识点 + 到期复习 */}
      <div className="pg-sec">
        <div className="pg-lab">今日待办</div>
        {today && today.week > 0 ? (
          <>
            <div className="pg-chips">
              <span className="pg-chip on">
                第 {today.week} / {today.total_weeks} 周
              </span>
              {today.stage_names.map((n) => (
                <span key={n} className="pg-chip">
                  {n}
                </span>
              ))}
            </div>
            {today.reviews.length > 0 && (
              <div className="rp-queries" style={{ marginTop: 8 }}>
                {today.reviews.map((r) => (
                  <span key={r.id} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b style={{ fontWeight: 500 }}>复习:{r.node_name}</b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>
                      {r.overdue ? '已过期' : '今日到期'} · 隔 {r.interval_days} 天
                    </i>
                    <button className="pg-chip" title="还记得:间隔晋级" onClick={() => onReview(r.node_id, true)}>
                      <IconCheck size={12} />
                      记得
                    </button>
                    <button className="pg-chip" title="忘了:回到 1 天后再复习" onClick={() => onReview(r.node_id, false)}>
                      忘了
                    </button>
                  </span>
                ))}
              </div>
            )}
            {today.nodes.length > 0 ? (
              <div className="rp-queries" style={{ marginTop: 8 }}>
                {today.nodes.map((n) => (
                  <span key={n.node_id} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b style={{ fontWeight: 500 }}>{n.node_name}</b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>D{n.difficulty}</i>
                    <button className="pg-chip" onClick={() => onLearn(n.node_id, n.node_name)}>
                      学习
                    </button>
                  </span>
                ))}
              </div>
            ) : (
              <div className="pg-hint">本周排期知识点都已完成;可提前学习后面阶段,或等复习到期。</div>
            )}
          </>
        ) : (
          <div className="pg-hint">暂无排期:计划生成后按周展开,此处显示当日应学与到期复习。</div>
        )}
      </div>

      {payload.stages.map((s) => {
        const res = payload.resources.find((r) => r.stage === s.index);
        const weeks = payload.schedule.filter((w) => w.stage === s.index);
        const isCurrent = !!today && today.stage_names.includes(s.name);
        return (
          <div key={s.index} className="rp-tl ok">
            <div className="tt">
              阶段 {s.index + 1}:{s.name}
              {isCurrent && <span className="pill">本周</span>}
              <span className="pill">难度 {s.difficulty}</span>
              <span className="pill">约 {s.estimated_weeks} 周</span>
            </div>
            {s.goal && <div className="dd">{s.goal}</div>}
            <div className="rp-queries">
              {s.topics.map((t) => {
                const node = payload.nodes.find((n) => n.name === t);
                const st = stateMap.get(t);
                return (
                  <span key={t} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                    <b
                      style={{
                        fontWeight: node?.must_learn ? 600 : 400,
                        color: node?.must_learn ? 'var(--text-1)' : 'var(--text-3)',
                      }}
                    >
                      {t}
                    </b>
                    <i style={{ fontStyle: 'normal', color: 'var(--text-4)' }}>D{node?.difficulty ?? '-'}</i>
                    {st?.done ? (
                      <span className="pill">已完成</span>
                    ) : st && st.mastery > 0 ? (
                      <span className="pill">掌握 {Math.round(st.mastery * 100)}%</span>
                    ) : null}
                    <button
                      className="pg-chip"
                      title="阅读该知识点的自学材料"
                      onClick={() => st && onLearn(st.node_id, t)}
                      disabled={!st}
                    >
                      学习
                    </button>
                    <button
                      className="pg-chip"
                      title="针对该知识点出 5 道测验题"
                      onClick={() => st && onQuiz(st.node_id, t)}
                      disabled={!st}
                    >
                      测验
                    </button>
                    <button
                      className="pg-chip"
                      title="针对该知识点发起一次深挖研究"
                      onClick={() => onDeepDig(t)}
                    >
                      <IconResearch size={13} />
                      深挖
                    </button>
                  </span>
                );
              })}
            </div>
            {s.milestone && (
              <div className="dd">
                <b>里程碑</b>:{s.milestone}
              </div>
            )}
            {weeks.length > 0 && (
              <div className="pg-hint">
                排期:第 {weeks[0].week}-{weeks[weeks.length - 1].week} 周 · 每天 {payload.daily_minutes} 分钟
              </div>
            )}
            {res && res.items.length > 0 && (
              <div className="rp-queries">
                {res.items.map((it, i) => (
                  <span key={i}>{it}</span>
                ))}
              </div>
            )}
          </div>
        );
      })}

      {/* 学习统计(P2):打卡 / 时长 / 完成度 + 专注计时 */}
      <StatsSec stats={stats} onChanged={onChanged} />
    </div>
  );
}
