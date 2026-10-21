import { Fragment, useMemo, useState } from 'react';
import type { ActiveResearch } from '../../stores/research';
import { CheckpointBar } from './CheckpointBar';
import type * as ipc from '../../lib/ipc';
import { IconChevronDown } from '../../components/icons';
import { buildTimeline } from './helpers';
import { useCardResize } from '../../lib/usePanelWidth';

export function StageTrail({
  active,
  checkpoint,
}: {
  active: ActiveResearch;
  checkpoint: { stage: number; next: string } | null;
}) {
  // 用户手动展开/收起覆盖(默认:进行中展开,其余收起)
  const [closed, setClosed] = useState<Record<string, boolean>>({});
  const cardRs = useCardResize();
  const groups = useMemo(() => {
    const m = new Map<number, ipc.ResearchEvent[]>();
    for (const ev of active.trail) {
      const s = 'stage' in ev && typeof ev.stage === 'number' ? ev.stage : 0;
      const arr = m.get(s);
      if (arr) arr.push(ev);
      else m.set(s, [ev]);
    }
    return m;
  }, [active.trail]);

  return (
    <>
      {active.stages.map((st, i) => {
        const no = i + 1;
        const evs = groups.get(no) ?? [];
        const nodes = buildTimeline(evs, st.status === 'running' ? active.status : 'done');
        const rounds = evs.filter((e) => e.type === 'round_start').length;
        const gap = [...evs].reverse().find((e) => e.type === 'angle_gap');
        const running = st.status === 'running';
        const open = closed[st.id] === undefined ? running : !closed[st.id];
        const isRe = st.kind === 'research';
        return (
          <Fragment key={st.id}>
            <div className={`trail-card${st.status === 'pending' ? ' wait' : ''}`}>
              <div
                className="trail-head"
                onClick={() => setClosed((m) => ({ ...m, [st.id]: open }))}
              >
                <span className={`stage-idx${isRe ? '' : ' syn'}`}>{no}</span>
                <span className="trail-title ellipsis">{st.name}</span>
                {!isRe && <span className="kind-badge sy">撰写</span>}
                {st.status === 'done' &&
                  (st.partial ? (
                    <span className="st-badge partial">
                      部分完成{rounds > 0 ? ` · ${rounds} 轮` : ''}
                    </span>
                  ) : (
                    <span className="st-badge done">
                      已完成{rounds > 0 ? ` · ${rounds} 轮` : ''}
                    </span>
                  ))}
                {running && <span className="st-badge run">进行中 · 第 {active.currentRound} 轮</span>}
                {st.status === 'pending' && <span className="st-badge wait">待执行</span>}
                {st.status === 'skipped' && <span className="st-badge skip">已跳过</span>}
                <span className="trail-hint ellipsis">
                  {st.status === 'done'
                    ? `新增资料 ${st.sources_count} 条`
                    : st.status === 'pending'
                      ? isRe
                        ? '等待前序阶段完成'
                        : '基于已有资料产出分析文本'
                      : ''}
                </span>
                <span className="arr" style={open ? { transform: 'rotate(180deg)' } : undefined}>
                  <IconChevronDown size={14} />
                </span>
              </div>
              {open && (
                <>
                  <div className="trail-body">
                  {st.summary && (
                    <div className="stage-summary">
                      <div className="ss-lab">阶段摘要</div>
                      <div className="ss-txt">{st.summary}</div>
                    </div>
                  )}
                  {gap && gap.type === 'angle_gap' && (
                    <div className="stage-gap">必覆盖角度缺失:{gap.missing.join('、')}(已提醒,不阻断)</div>
                  )}
                  {nodes.length === 0 && !st.summary && (
                    <div className="rp-empty" style={{ padding: 'var(--space-3)' }}>
                      {st.status === 'skipped' ? '本阶段已跳过' : '暂无过程记录'}
                    </div>
                  )}
                  {nodes.map((n) => (
                    <div key={n.key} className={`rp-tl ${n.state}`}>
                      <div className="tt">
                        {n.title}
                        <span className={`pill${n.state === 'doing' ? ' warn' : ''}`}>
                          {n.state === 'doing' ? '运行中' : '完成'}
                        </span>
                      </div>
                      {n.queries.length > 0 && (
                        <div className="rp-queries">
                          {n.queries.map((q, qi) => (
                            <span key={qi}>{q}</span>
                          ))}
                        </div>
                      )}
                      {n.dd && (
                        <>
                          <div className="dd">{n.dd}</div>
                          <div
                            className="card-resize"
                            title="拖拽调整高度,双击复位"
                            onPointerDown={cardRs.onDown}
                            onPointerMove={cardRs.onMove}
                            onPointerUp={cardRs.onUp}
                            onDoubleClick={cardRs.onDoubleClick}
                          />
                        </>
                      )}
                    </div>
                  ))}
                  </div>
                  <div
                    className="card-resize"
                    title="拖拽调整本阶段卡高度,双击复位"
                    onPointerDown={cardRs.onDown}
                    onPointerMove={cardRs.onMove}
                    onPointerUp={cardRs.onUp}
                    onDoubleClick={cardRs.onDoubleClick}
                  />
                </>
              )}
            </div>
            {checkpoint && checkpoint.stage === no && (
              <CheckpointBar stage={no} next={checkpoint.next} />
            )}
          </Fragment>
        );
      })}
      {active.stages.every((s) => s.status === 'pending') && active.trail.length === 0 && (
        <div className="rp-empty">正在启动研究…</div>
      )}
    </>
  );
}




