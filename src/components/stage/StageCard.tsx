import { useState } from 'react';
import * as ipc from '../../lib/ipc';
import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconChevronDown,
  IconEdit,
  IconPause,
  IconRestore,
  IconTrash,
} from '../icons';
import { StageResearchFields } from './StageResearchFields';

export function StageCard({
  st,
  idx,
  total,
  expanded,
  onToggle,
  onPatch,
  onRemove,
  onMove,
}: {
  st: ipc.StageConfig;
  idx: number;
  total: number;
  expanded: boolean;
  onToggle: () => void;
  onPatch: (p: Partial<ipc.StageConfig>) => void;
  onRemove: () => void;
  onMove: (dir: -1 | 1) => void;
}) {
  const isRe = st.kind === 'research';
  const [promptEdit, setPromptEdit] = useState<'decide' | 'summary' | null>(null);
  const patchC = (p: Partial<ipc.StageConstraints>) =>
    onPatch({ constraints: { ...st.constraints, ...p } });

  const cls = `stage-card${expanded ? ' on' : ''}${st.enabled ? '' : ' off'}`;
  return (
    <div className={cls}>
      <div className="stage-card-h" onClick={onToggle}>
        <span className={`stage-idx${isRe ? '' : ' syn'}`}>{idx + 1}</span>
        <span className="nm" title={st.name}>
          {st.name || `阶段 ${idx + 1}`}
          {!st.enabled && '(已停用)'}
        </span>
        <span className={`kind-badge ${isRe ? 're' : 'sy'}`}>{isRe ? '检索' : '撰写'}</span>
        <span
          className="hbtn"
          title="上移"
          style={{ visibility: idx === 0 ? 'hidden' : undefined }}
          onClick={(e) => {
            e.stopPropagation();
            onMove(-1);
          }}
        >
          <IconArrowUp size={13} />
        </span>
        <span
          className="hbtn"
          title="下移"
          style={{ visibility: idx === total - 1 ? 'hidden' : undefined }}
          onClick={(e) => {
            e.stopPropagation();
            onMove(1);
          }}
        >
          <IconArrowDown size={13} />
        </span>
        <span
          className="hbtn"
          title={st.enabled ? '停用' : '启用'}
          onClick={(e) => {
            e.stopPropagation();
            onPatch({ enabled: !st.enabled });
          }}
        >
          {st.enabled ? <IconPause size={12} /> : <IconRestore size={13} />}
        </span>
        <span
          className="hbtn del"
          title="删除阶段"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <IconTrash size={13} />
        </span>
        <span className="arr" style={expanded ? { transform: 'rotate(180deg)' } : undefined}>
          <IconChevronDown size={14} />
        </span>
      </div>

      {expanded && (
        <div className="stage-card-b" onClick={(e) => e.stopPropagation()}>
          <div className="ov-row">
            <span className="lbl">阶段名称</span>
            <input
              className="pg-field"
              value={st.name}
              placeholder={`阶段 ${idx + 1}`}
              onChange={(e) => onPatch({ name: e.target.value })}
            />
          </div>
          <div className="ov-row">
            <span className="lbl">阶段类型</span>
            <div className="pg-seg" style={{ flex: 1 }}>
              <button className={isRe ? 'on' : ''} onClick={() => onPatch({ kind: 'research' })}>
                检索
              </button>
              <button className={!isRe ? 'on' : ''} onClick={() => onPatch({ kind: 'synthesis' })}>
                撰写
              </button>
            </div>
          </div>
          <div className="ov-row">
            <span className="lbl">阶段目标</span>
            <textarea
              className="pg-field"
              rows={2}
              placeholder="本阶段要达成什么?将注入决策上下文引导检索方向"
              value={st.goal}
              onChange={(e) => onPatch({ goal: e.target.value })}
            />
          </div>

          {isRe && <StageResearchFields st={st} onPatch={onPatch} patchC={patchC} />}

          <div className="ov-row">
            <span
              className={`tool-chip${st.constraints.inject_prev_summaries ? ' on' : ''}`}
              onClick={() => patchC({ inject_prev_summaries: !st.constraints.inject_prev_summaries })}
            >
              {st.constraints.inject_prev_summaries && <IconCheck size={12} />}
              注入前阶段摘要
            </span>
            {isRe && (
              <span
                className={`tool-chip${st.decide_prompt ? ' on' : ''}${promptEdit === 'decide' ? ' on' : ''}`}
                onClick={() => setPromptEdit(promptEdit === 'decide' ? null : 'decide')}
              >
                <IconEdit size={12} />
                阶段决策提示覆盖
              </span>
            )}
            <span
              className={`tool-chip${st.summary_prompt ? ' on' : ''}${promptEdit === 'summary' ? ' on' : ''}`}
              onClick={() => setPromptEdit(promptEdit === 'summary' ? null : 'summary')}
            >
              <IconEdit size={12} />
              {isRe ? '阶段摘要提示覆盖' : '撰写提示覆盖'}
            </span>
          </div>
          {promptEdit === 'decide' && isRe && (
            <div className="stage-prompt-ed">
              <textarea
                className="pg-field"
                rows={4}
                placeholder="留空 = 跟随全局决策提示;可用占位符 {topic} {stage_name} {stage_goal} {round} {max_rounds} {queries_per_round} {must_cover_angles} {query_language} {prev_summaries}"
                value={st.decide_prompt ?? ''}
                onChange={(e) => onPatch({ decide_prompt: e.target.value || null })}
              />
            </div>
          )}
          {promptEdit === 'summary' && (
            <div className="stage-prompt-ed">
              <textarea
                className="pg-field"
                rows={4}
                placeholder="留空 = 跟随内置摘要/撰写提示;可用占位符 {topic} {stage_name} {stage_goal} {sources_digest} {prev_summaries} {output_lang}"
                value={st.summary_prompt ?? ''}
                onChange={(e) => onPatch({ summary_prompt: e.target.value || null })}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
