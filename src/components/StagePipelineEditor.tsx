/**
 * 阶段流水线编辑器(专项研究,步骤 2)。
 *
 * 视觉/结构严格遵循 plan/07-specialized-research-pages.html p1 L397-495:
 *   - stage-card 收起/展开、增删、上移下移(箭头按钮,不用 HTML5 DnD)、启停
 *   - 参数覆盖 inherit-tag「跟随全局」占位态,点击切换自定义
 *   - 阶段内约束 gc-grid / 必覆盖角度 / 域名黑名单 / 查询语言 / tool-chip 开关
 *   - 「撰写」型阶段仅保留名称/目标/摘要提示覆盖(后端 run_synthesis 只消费这些)
 */
import { useEffect, useState } from 'react';
import * as ipc from '../lib/ipc';
import {
  IconChevronDown,
  IconArrowUp,
  IconArrowDown,
  IconTrash,
  IconRestore,
  IconPlus,
  IconX,
  IconCheck,
  IconEdit,
  IconPause,
} from './icons';

interface Props {
  stages: ipc.StageConfig[];
  onChange: (stages: ipc.StageConfig[]) => void;
}

const SOURCE_MODES = [
  { id: 'web', label: '全网' },
  { id: 'academic', label: '学术优先' },
  { id: 'whitelist', label: '白名单' },
];

const LANG_MODES = [
  { id: 'auto', label: '自动' },
  { id: 'zh', label: '中文' },
  { id: 'en', label: 'English' },
];

/** 逗号分隔文本输入:本地暂存,失焦/回车时解析提交(避免输入途中被截断)。 */
function CsvInput({
  value,
  placeholder,
  onCommit,
}: {
  value: string[];
  placeholder?: string;
  onCommit: (arr: string[]) => void;
}) {
  const [text, setText] = useState(value.join(', '));
  useEffect(() => setText(value.join(', ')), [value]);
  const commit = () =>
    onCommit(
      text
        .split(/[,,、\s]+/)
        .map((s) => s.trim())
        .filter(Boolean),
    );
  return (
    <input
      className="pg-field"
      value={text}
      placeholder={placeholder}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        }
      }}
    />
  );
}

/** 数字覆盖 chip:null = 跟随全局;点击切换自定义。 */
function OvNum({
  label,
  value,
  def,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number | null;
  def: number;
  min: number;
  max: number;
  onChange: (v: number | null) => void;
}) {
  if (value === null) {
    return (
      <span className="inherit-tag" title="点击覆盖全局值" onClick={() => onChange(def)}>
        {label} · 跟随全局
      </span>
    );
  }
  return (
    <span className="inherit-tag custom">
      {label}
      <input
        type="number"
        className="ov-num"
        min={min}
        max={max}
        value={value}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => onChange(Math.max(min, Math.min(max, Number(e.target.value) || def)))}
      />
      <i className="x" title="恢复跟随全局" onClick={() => onChange(null)}>
        <IconX size={10} />
      </i>
    </span>
  );
}

/** 选择覆盖 chip:null = 跟随全局;点击循环选项,右键或 × 恢复。 */
function OvSel({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: { id: string; label: string }[];
  onChange: (v: string | null) => void;
}) {
  if (value === null) {
    return (
      <span className="inherit-tag" title="点击覆盖全局值" onClick={() => onChange(options[0].id)}>
        {label} · 跟随全局
      </span>
    );
  }
  return (
    <span className="inherit-tag custom">
      {label}
      <select value={value} onClick={(e) => e.stopPropagation()} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
      <i className="x" title="恢复跟随全局" onClick={() => onChange(null)}>
        <IconX size={10} />
      </i>
    </span>
  );
}

/** 约束数字输入:null = 不限(占位提示)。 */
function GcNum({
  label,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  value: number | null;
  placeholder: string;
  onChange: (v: number | null) => void;
}) {
  return (
    <div className="gc-item">
      <span className="l">{label}</span>
      <input
        className="pg-field num"
        type="number"
        min={0}
        placeholder={placeholder}
        value={value ?? ''}
        onChange={(e) => {
          const t = e.target.value.trim();
          onChange(t === '' ? null : Math.max(0, Number(t) || 0));
        }}
      />
    </div>
  );
}

function StageCard({
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

          {isRe && (
            <>
              <div className="ov-row">
                <span className="lbl">参数覆盖</span>
                <OvNum
                  label="轮次"
                  value={st.max_rounds}
                  def={4}
                  min={1}
                  max={25}
                  onChange={(v) => onPatch({ max_rounds: v })}
                />
                <OvNum
                  label="每轮查询"
                  value={st.queries_per_round}
                  def={4}
                  min={1}
                  max={10}
                  onChange={(v) => onPatch({ queries_per_round: v })}
                />
                <OvSel
                  label="阅读长度"
                  value={st.read_length}
                  options={[
                    { id: 'digest', label: '摘要' },
                    { id: 'full', label: '扩展' },
                  ]}
                  onChange={(v) => onPatch({ read_length: v })}
                />
                <OvSel
                  label="反思策略"
                  value={st.reflect}
                  options={[
                    { id: 'every', label: '每轮' },
                    { id: 'every3', label: '每 3 轮' },
                    { id: 'final', label: '仅开头' },
                  ]}
                  onChange={(v) => onPatch({ reflect: v })}
                />
              </div>
              <div className="ov-row">
                <span className="lbl">信息源</span>
                <div className="pg-seg" style={{ flex: 1 }}>
                  {SOURCE_MODES.map((m) => (
                    <button
                      key={m.id}
                      className={st.source_mode === m.id ? 'on' : ''}
                      title={st.source_mode === m.id ? '再次点击恢复跟随全局' : undefined}
                      onClick={() => onPatch({ source_mode: st.source_mode === m.id ? null : m.id })}
                    >
                      {m.label}
                    </button>
                  ))}
                </div>
                {st.source_mode === null && <span className="inherit-tag">跟随全局</span>}
              </div>
              {st.source_mode === 'whitelist' && (
                <div className="ov-row">
                  <span className="lbl">白名单域名</span>
                  <CsvInput
                    value={st.whitelist_domains ?? []}
                    placeholder="例:arxiv.org, github.com(留空回退全局白名单)"
                    onCommit={(arr) => onPatch({ whitelist_domains: arr })}
                  />
                </div>
              )}

              <div className="pg-lab" style={{ margin: 'var(--space-1) 0 0' }}>
                阶段内约束
              </div>
              <div className="gc-grid">
                <GcNum
                  label="Token 上限"
                  value={st.constraints.stage_token_budget}
                  placeholder="不限"
                  onChange={(v) => patchC({ stage_token_budget: v })}
                />
                <GcNum
                  label="检索次数上限"
                  value={st.constraints.stage_max_searches}
                  placeholder="不限"
                  onChange={(v) => patchC({ stage_max_searches: v })}
                />
                <GcNum
                  label="资料数下限"
                  value={st.constraints.min_sources}
                  placeholder="不校验"
                  onChange={(v) => patchC({ min_sources: v })}
                />
                <GcNum
                  label="新增资料上限"
                  value={st.constraints.stage_max_sources}
                  placeholder="不限"
                  onChange={(v) => patchC({ stage_max_sources: v })}
                />
              </div>
              <div className="ov-row">
                <span className="lbl">必覆盖角度</span>
                <CsvInput
                  value={st.constraints.must_cover_angles}
                  placeholder="例:推理速度, 内存占用(阶段结束校验)"
                  onCommit={(arr) => patchC({ must_cover_angles: arr })}
                />
                {st.constraints.must_cover_angles.length > 0 && (
                  <span className="inherit-tag custom">缺失将提醒</span>
                )}
              </div>
              <div className="ov-row">
                <span className="lbl">域名黑名单</span>
                <CsvInput
                  value={st.constraints.exclude_domains}
                  placeholder="例:zhihu.com, csdn.net(检索结果过滤)"
                  onCommit={(arr) => patchC({ exclude_domains: arr })}
                />
              </div>
              <div className="ov-row">
                <span className="lbl">查询语言</span>
                <div className="pg-seg" style={{ flex: 1 }}>
                  {LANG_MODES.map((m) => (
                    <button
                      key={m.id}
                      className={st.constraints.query_language === m.id ? 'on' : ''}
                      onClick={() => patchC({ query_language: m.id })}
                    >
                      {m.label}
                    </button>
                  ))}
                </div>
              </div>
              <div className="ov-row">
                <span
                  className={`tool-chip${st.constraints.early_stop_on_enough ? ' on' : ''}`}
                  onClick={() => patchC({ early_stop_on_enough: !st.constraints.early_stop_on_enough })}
                >
                  {st.constraints.early_stop_on_enough && <IconCheck size={12} />}
                  资料足够可提前结束
                </span>
                <span
                  className={`tool-chip${st.constraints.inherit_sources ? ' on' : ''}`}
                  onClick={() => patchC({ inherit_sources: !st.constraints.inherit_sources })}
                >
                  {st.constraints.inherit_sources && <IconCheck size={12} />}
                  继承前阶段资料明细
                </span>
              </div>
            </>
          )}

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

export function StagePipelineEditor({ stages, onChange }: Props) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const enabled = stages.filter((s) => s.enabled).length;

  const patchAt = (i: number, p: Partial<ipc.StageConfig>) =>
    onChange(stages.map((s, j) => (j === i ? { ...s, ...p } : s)));
  const removeAt = (i: number) => onChange(stages.filter((_, j) => j !== i));
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= stages.length) return;
    const next = [...stages];
    [next[i], next[j]] = [next[j], next[i]];
    onChange(next);
  };
  const addStage = (kind: string) => {
    const st = ipc.newStage('', kind);
    onChange([...stages, st]);
    setExpandedId(st.id);
  };

  return (
    <div className="stage-pipe">
      <div className="pg-lab">
        阶段流水线
        <span className="sp" />
        <span className="n">
          {stages.length} 阶段 · {enabled} 启用
        </span>
      </div>
      {stages.map((st, i) => (
        <div key={st.id}>
          {i > 0 && (
            <div className="stage-conn">
              <IconArrowDown size={14} />
            </div>
          )}
          <StageCard
            st={st}
            idx={i}
            total={stages.length}
            expanded={expandedId === st.id}
            onToggle={() => setExpandedId(expandedId === st.id ? null : st.id)}
            onPatch={(p) => patchAt(i, p)}
            onRemove={() => removeAt(i)}
            onMove={(d) => move(i, d)}
          />
        </div>
      ))}
      <div className="stage-add">
        <button className="btn-ghost" onClick={() => addStage('research')}>
          <IconPlus size={13} />
          检索阶段
        </button>
        <button className="btn-ghost" onClick={() => addStage('synthesis')}>
          <IconPlus size={13} />
          撰写阶段
        </button>
      </div>
      <div className="pg-hint">
        「撰写」型阶段不检索,仅基于已有资料产出分析文本;终稿在全部阶段后自动生成。
      </div>
    </div>
  );
}
