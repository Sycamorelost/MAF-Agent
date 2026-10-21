import * as ipc from '../../lib/ipc';
import { IconCheck } from '../icons';
import { CsvInput, GcNum, LANG_MODES, OvNum, OvSel, SOURCE_MODES } from './OvFields';

/** 检索阶段的专属字段:参数覆盖 / 信息源 / 阶段内约束 / 必覆盖角度 / 域名 / 语言 / 开关。 */
export function StageResearchFields({
  st,
  onPatch,
  patchC,
}: {
  st: ipc.StageConfig;
  onPatch: (p: Partial<ipc.StageConfig>) => void;
  patchC: (p: Partial<ipc.StageConstraints>) => void;
}) {
  return (
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
  );
}
