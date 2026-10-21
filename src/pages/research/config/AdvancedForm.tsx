import * as ipc from '../../../lib/ipc';
import { IconCheck, IconDoc, IconEdit } from '../../../components/icons';
import { StagePipelineEditor } from '../../../components/stage/StagePipelineEditor';

/** 高级表单:全局约束 + 专项研究的阶段流水线。 */
export function AdvancedForm({
  params,
  templates,
  searchCfg,
  engineReady,
  onPatch,
  onOpenTpl,
}: {
  params: ipc.ResearchParams;
  templates: ipc.ResearchTemplateDto[];
  searchCfg: ipc.SearchConfigDto | null;
  engineReady: boolean;
  onPatch: (p: Partial<ipc.ResearchParams>) => void;
  onOpenTpl: (key: string) => void;
}) {
  return (
    <>
      <div className="pg-sec">
        <div className="pg-lab">
          全局约束
          <span className="sp" />
          <span className="n">触发任一即收尾</span>
        </div>
        <div className="gc-grid">
          <div className="gc-item">
            <span className="l">总 Token 预算</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              step={10000}
              title="0 = 不限"
              value={params.token_budget}
              onChange={(e) =>
                onPatch({ token_budget: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
          <div className="gc-item">
            <span className="l">总检索次数 (0 不限)</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              max={200}
              value={params.max_searches}
              onChange={(e) => onPatch({ max_searches: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          {params.mode === 'staged' && (
            <>
              <div className="gc-item">
                <span className="l">总时长·分钟 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={600}
                  value={params.max_duration_minutes}
                  onChange={(e) =>
                    onPatch({ max_duration_minutes: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">总资料上限 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={500}
                  value={params.max_sources}
                  onChange={(e) =>
                    onPatch({ max_sources: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">查询并发 1-5</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={1}
                  max={5}
                  value={params.concurrent_queries}
                  onChange={(e) =>
                    onPatch({
                      concurrent_queries: Math.max(1, Math.min(5, Number(e.target.value) || 1)),
                    })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">单查询失败策略</span>
                <select
                  className="pg-field"
                  value={params.fail_policy}
                  onChange={(e) => onPatch({ fail_policy: e.target.value })}
                >
                  <option value="skip">跳过继续</option>
                  <option value="abort_stage">中止本阶段</option>
                  <option value="abort_research">中止研究</option>
                </select>
              </div>
              <div className="gc-item">
                <span className="l">URL 去重范围</span>
                <select
                  className="pg-field"
                  value={params.dedup_scope}
                  onChange={(e) => onPatch({ dedup_scope: e.target.value })}
                >
                  <option value="global">全程去重</option>
                  <option value="stage">仅阶段内</option>
                </select>
                {params.dedup_scope === 'stage' && (
                  <div className="pg-hint">分阶段去重下,资料上限仍按全局累计。</div>
                )}
              </div>
            </>
          )}
          <div className="gc-item">
            <span className="l">输出语言</span>
            <select
              className="pg-field"
              value={params.output_lang}
              onChange={(e) => onPatch({ output_lang: e.target.value })}
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
            <button
              type="button"
              className={`tool-chip${params.checkpoint ? ' on' : ''}`}
              onClick={() => onPatch({ checkpoint: !params.checkpoint })}
            >
              <IconCheck size={12} />
              检查点模式
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
              onClick={() => onOpenTpl('decide')}
            >
              <IconEdit size={12} />
              全局决策提示
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
              onClick={() => onOpenTpl('report')}
            >
              <IconDoc size={12} />
              终稿模板
            </button>
            <button
              type="button"
              className={`tool-chip${params.free_structure ? ' on' : ''}`}
              title="不使用报告模板,AI 根据主题与资料自行决定报告章节结构"
              onClick={() => onPatch({ free_structure: !params.free_structure })}
            >
              <IconEdit size={12} />
              自由结构
            </button>
          </div>
        )}
        <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
          <span>搜索引擎</span>
          <span style={{ color: engineReady ? 'var(--ok)' : 'var(--warn)' }}>
            {searchCfg ? (engineReady ? '已就绪' : '未就绪') : '加载中…'}
          </span>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-hint">
            检查点:每阶段结束暂停,可查看摘要、调整下阶段配置后再继续;高亮模板表示已自定义。
          </div>
        )}
      </div>

      {params.mode === 'staged' && (
        <div className="pg-sec">
          <StagePipelineEditor stages={params.stages} onChange={(stages) => onPatch({ stages })} />
        </div>
      )}
    </>
  );
}
