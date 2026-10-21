import * as ipc from '../../../lib/ipc';
import { DEPTH_PRESETS, SOURCE_MODES } from '../constants';

/** 基础表单:研究模式 / 主题 / 深度 / 信息源 / 执行模型。 */
export function BasicForm({
  params,
  topic,
  isCustom,
  whitelistText,
  modelSel,
  templatesProviders,
  hasKey,
  current,
  onPatch,
  onIsCustomChange,
  onTopicChange,
  onWhitelistTextChange,
  onModelSelChange,
}: {
  params: ipc.ResearchParams;
  topic: string;
  isCustom: boolean;
  whitelistText: string;
  modelSel: string;
  templatesProviders: ipc.ProviderTemplate[];
  hasKey: Record<string, boolean>;
  current: { providerId: string; model: string } | null;
  onPatch: (p: Partial<ipc.ResearchParams>) => void;
  onIsCustomChange: (v: boolean) => void;
  onTopicChange: (v: string) => void;
  onWhitelistTextChange: (v: string) => void;
  onModelSelChange: (v: string) => void;
}) {
  // 执行模型下拉:跟随当前 + 全部已配 key 的 provider×model
  const modelOptions: { value: string; label: string }[] = [];
  for (const t of templatesProviders) {
    if (!hasKey[t.id]) continue;
    for (const m of t.models) {
      modelOptions.push({ value: `${t.id}::${m}`, label: `${t.display_name} · ${m}` });
    }
  }

  const modelLabel = current
    ? `${templatesProviders.find((t) => t.id === current.providerId)?.display_name ?? current.providerId} · ${current.model}`
    : '未选择';

  return (
    <>
      <div className="pg-sec">
        <div className="pg-lab">研究模式</div>
        <div className="mode-seg">
          <button
            className={params.mode === 'concise' ? 'on' : ''}
            onClick={() => onPatch({ mode: 'concise' })}
          >
            简洁研究
          </button>
          <button
            className={params.mode === 'legacy' ? 'on' : ''}
            onClick={() => onPatch({ mode: 'legacy' })}
          >
            快速研究
          </button>
          <button
            className={params.mode === 'staged' ? 'on' : ''}
            onClick={() =>
              onPatch(
                params.stages.length > 0
                  ? { mode: 'staged' }
                  : { mode: 'staged', stages: ipc.builtinStages() },
              )
            }
          >
            专项研究
          </button>
        </div>
        {params.mode === 'concise' && (
          <div className="pg-hint">
            简洁模式单对话多轮迭代:第 1 轮输出完整初稿,之后每轮自评上一稿不足 → 轻量检索补充 → 整稿重写,逐轮更准。
          </div>
        )}
        {params.mode === 'staged' && (
          <div className="pg-hint">
            专项模式按「阶段流水线」执行:每阶段独立参数与约束,前一阶段结论自动注入后续阶段。
          </div>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究主题</div>
        <textarea
          className="pg-area"
          rows={3}
          placeholder="想研究什么?例如:2026 年端侧小模型推理优化方案对比"
          value={topic}
          onChange={(e) => onTopicChange(e.target.value)}
        />
      </div>

      <div className="pg-sec">
        <div className="pg-lab">{params.mode === 'concise' ? '迭代轮次' : '研究深度'}</div>
        {params.mode === 'concise' ? (
          <div className="pg-row">
            <span>轮数</span>
            <input
              className="pg-field num"
              type="number"
              min={1}
              max={10}
              value={params.max_rounds}
              onChange={(e) =>
                onPatch({ max_rounds: Math.max(1, Math.min(10, Number(e.target.value) || 1)) })
              }
            />
          </div>
        ) : (
          <>
            <div className="pg-seg joined">
              {DEPTH_PRESETS.map((p) => (
                <button
                  key={p.rounds}
                  className={!isCustom && params.max_rounds === p.rounds ? 'on' : ''}
                  onClick={() => {
                    onIsCustomChange(false);
                    onPatch({ max_rounds: p.rounds });
                  }}
                >
                  {p.label}
                </button>
              ))}
              <button className={isCustom ? 'on' : ''} onClick={() => onIsCustomChange(true)}>
                自定义
              </button>
            </div>
            {isCustom && (
              <div className="pg-row">
                <span>轮次</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={1}
                  max={25}
                  value={params.max_rounds}
                  onChange={(e) =>
                    onPatch({ max_rounds: Math.max(1, Math.min(25, Number(e.target.value) || 1)) })
                  }
                />
              </div>
            )}
          </>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">信息源</div>
        <div className="pg-chips">
          {SOURCE_MODES.map((m) => (
            <button
              key={m.id}
              className={`pg-chip${params.source_mode === m.id ? ' on' : ''}`}
              onClick={() => onPatch({ source_mode: m.id })}
            >
              {m.label}
            </button>
          ))}
        </div>
        {params.source_mode === 'whitelist' && (
          <input
            className="pg-field"
            style={{ width: '100%', marginTop: 'var(--space-2)' }}
            placeholder="域名,逗号分隔,如 arxiv.org, github.com"
            value={whitelistText}
            onChange={(e) => onWhitelistTextChange(e.target.value)}
          />
        )}
        <div className="pg-row">
          <span>每轮查询数</span>
          <select
            className="pg-field"
            value={params.queries_per_round}
            onChange={(e) => onPatch({ queries_per_round: Number(e.target.value) })}
          >
            {[2, 3, 4, 5, 6, 8].map((n) => (
              <option key={n} value={n}>
                {n} 条
              </option>
            ))}
          </select>
        </div>
        <div className="pg-row">
          <span>单源阅读长度</span>
          <select
            className="pg-field"
            value={params.read_length}
            onChange={(e) => onPatch({ read_length: e.target.value })}
          >
            <option value="digest">摘要(240 字)</option>
            <option value="full">扩展(≤1200 字)</option>
          </select>
        </div>
        <div className="pg-row">
          <span>反思策略</span>
          <select
            className="pg-field"
            value={params.reflect}
            onChange={(e) => onPatch({ reflect: e.target.value })}
          >
            <option value="every">每轮反思</option>
            <option value="every3">每 3 轮</option>
            <option value="final">仅开头规划</option>
          </select>
        </div>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">执行模型</div>
        <select
          className="pg-field"
          style={{ width: '100%' }}
          value={modelSel}
          onChange={(e) => onModelSelChange(e.target.value)}
          title={modelSel ? undefined : modelLabel}
        >
          <option value="">跟随当前 · {modelLabel}</option>
          {modelOptions.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
    </>
  );
}
