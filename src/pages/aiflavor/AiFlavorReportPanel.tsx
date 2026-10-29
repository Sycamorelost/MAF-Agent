import { useMemo } from 'react';
import * as ipc from '../../lib/ipc';
import { REWRITE_LEVELS } from './utils';
import { Gauge } from './components/Gauge';
import { Radar } from './components/Radar';

/** 中栏:检测报告(评分卡 / 六维 / 热力图 / 命中规则 / 改写档位)。 */
export interface AiFlavorReportPanelProps {
  report: ipc.AiflavorReport | null;
  focusSent: number | null;
  onLocateRule: (r: ipc.AiflavorRuleHit) => void;
  rewriteLevel: string;
  onLevelChange: (id: string) => void;
  anchor: string;
  rewriting: boolean;
  onRewrite: () => void;
}

export function AiFlavorReportPanel({
  report,
  focusSent,
  onLocateRule,
  rewriteLevel,
  onLevelChange,
  anchor,
  rewriting,
  onRewrite,
}: AiFlavorReportPanelProps) {
  /** 报告结论:偏离最大的维度 + 中度以上句子数(评分卡一眼可读)。 */
  const worstDim = useMemo(
    () => (report ? [...report.dims].sort((a, b) => b.score - a.score)[0] ?? null : null),
    [report],
  );
  const hotSent = report ? report.sentence_marks.filter((s) => s.heat >= 2).length : 0;

  return (
    <section className="aif-col aif-mid">
      {!report ? (
        <div className="aif-card aif-empty">在左侧输入文本并点击「开始检测」。<br />
          检测完全在本地运行,零模型调用、不上传任何内容。</div>
      ) : (
        <>
          <div className="aif-card aif-scorecard">
            <Gauge score={report.score} />
            <div className="aif-score-side">
              <div className="aif-score-top">
                <span className={`aif-level lv${report.level_code}`}>{report.level}</span>
                <span className="aif-score-note">AI 味指数 {report.score}/100</span>
              </div>
              <div className="aif-stat-line">
                {report.char_count} 字 · {report.stats.sentence_count} 句 ·{' '}
                {report.stats.paragraph_count} 段
              </div>
              {report.stats.academic_style && (
                <div className="aif-stat-line ok">已识别学术文体:具体性维度放宽</div>
              )}
              {report.degraded && (
                <div className="aif-stat-line warn">短文(≤100 字):结果仅供参考</div>
              )}
              {worstDim && (
                <div className="aif-score-verdict">
                  首要偏离:<b>{worstDim.name}</b>({worstDim.score}/100) · 中度以上句子{' '}
                  <b>{hotSent}</b>/{report.sentence_marks.length}
                </div>
              )}
            </div>
            <Radar dims={report.dims} />
          </div>

          <div className="aif-card">
            <div className="aif-card-head">
              <div className="aif-card-title">六维指标</div>
              <span className="aif-hint">越低越接近人味</span>
            </div>
            <div className="aif-dims">
              {report.dims.map((d) => (
                <div
                  key={d.key}
                  className={`aif-dim${worstDim?.key === d.key ? ' top' : ''}`}
                  title={d.example}
                >
                  <div className="aif-dim-head">
                    <span className="aif-dim-name">{d.name}</span>
                    <span className="aif-dim-value">
                      {d.score}
                      <small>/100 · {d.raw}</small>
                    </span>
                  </div>
                  <div className="aif-bar">
                    <i style={{ width: `${d.score}%` }} data-lv={d.level} />
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="aif-card">
            <div className="aif-card-head">
              <div className="aif-card-title">句子热力图</div>
              <div className="aif-legend">
                <i style={{ background: 'transparent', border: '1px solid var(--border)' }} />人味
                <i style={{ background: 'rgba(230,126,34,.18)' }} />轻度
                <i style={{ background: 'rgba(231,76,60,.30)' }} />中度
                <i style={{ background: 'rgba(192,57,43,.50)' }} />高度
              </div>
            </div>
            <div className="aif-heat">
              {report.sentence_marks.map((s) => (
                <span
                  key={s.index}
                  id={`aif-sent-${s.index}`}
                  className={`aif-sent${focusSent === s.index ? ' focus' : ''}`}
                  data-heat={s.heat}
                  title={s.reasons.length ? `命中:${s.reasons.join(';')}` : undefined}
                  style={{ background: ipc.HEAT_COLORS[s.heat] }}
                >
                  {s.text}
                </span>
              ))}
            </div>
          </div>

          {report.rule_hits.length > 0 && (
            <div className="aif-card">
              <div className="aif-card-head">
                <div className="aif-card-title">命中规则({report.rule_hits.length})</div>
                <span className="aif-hint">点击某行定位句子</span>
              </div>
              <div className="aif-rules">
                {report.rule_hits.map((r) => (
                  <div
                    key={r.rule_id}
                    className="aif-rule clickable"
                    title="点击定位到对应句子"
                    onClick={() => onLocateRule(r)}
                  >
                    <div className="aif-rule-main">
                      <span className={`aif-sev s${r.severity}`}>L{r.severity}</span>
                      <b>{r.name}</b>
                      <span className="aif-rule-cat">{r.category}</span>
                      <span className="aif-rule-count">{r.count} 次</span>
                    </div>
                    <span className="aif-rule-ex" title={r.example}>
                      {r.example}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="aif-card">
            <div className="aif-card-title">开始改写</div>
            <div className="aif-levels">
              {REWRITE_LEVELS.map((l) => (
                <label key={l.id} className={`aif-lvl${rewriteLevel === l.id ? ' on' : ''}`}>
                  <input
                    type="radio"
                    name="aif-lvl"
                    checked={rewriteLevel === l.id}
                    onChange={() => onLevelChange(l.id)}
                  />
                  <b>{l.label}</b>
                  <span>{l.desc}</span>
                </label>
              ))}
            </div>
            {rewriteLevel === 'deep' && !anchor.trim() && (
              <div className="aif-hint warn">深度档需要风格锚点:请在左侧填写旧文样本。</div>
            )}
            <button
              type="button"
              className="btn primary aif-cta"
              disabled={rewriting}
              onClick={() => onRewrite()}
            >
              {rewriting ? '改写中…(右侧实时输出)' : '开始改写 →'}
            </button>
          </div>

          <div className="aif-disclaimer">{report.disclaimer}</div>
        </>
      )}
    </section>
  );
}
