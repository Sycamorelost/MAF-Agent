import { useMemo } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import * as ipc from '../../lib/ipc';
import { MemoMarkdown } from '../../components/MemoMarkdown';
import { PILL_AGENTS, splitSentences, diffSentences, extractPolished } from './utils';
import { buildDisclosure } from './aiflavorHelpers';

/** 右栏:智能体输出(改写对照 / 各智能体结果)与学术披露生成器。 */
export interface AiFlavorOutputPanelProps {
  rightTab: 'rewrite' | 'academic';
  setRightTab: Dispatch<SetStateAction<'rewrite' | 'academic'>>;
  activeAgent: string;
  text: string;
  rewriteOut: string;
  rewriting: boolean;
  rewriteView: 'diff' | 'raw';
  setRewriteView: Dispatch<SetStateAction<'diff' | 'raw'>>;
  extractedRewrite: string;
  report: ipc.AiflavorReport | null;
  agentOut: string;
  agentRunning: boolean;
  discModel: string;
  setDiscModel: Dispatch<SetStateAction<string>>;
  discUse: string;
  setDiscUse: Dispatch<SetStateAction<string>>;
  onApplyRewrite: () => void;
  onApplyPolished: () => void;
  onApplyAnchor: () => void;
  onRunAgent: () => void;
  onStop: () => void;
  onMessage: (m: string) => void;
}

export function AiFlavorOutputPanel({
  rightTab,
  setRightTab,
  activeAgent,
  text,
  rewriteOut,
  rewriting,
  rewriteView,
  setRewriteView,
  extractedRewrite,
  report,
  agentOut,
  agentRunning,
  discModel,
  setDiscModel,
  discUse,
  setDiscUse,
  onApplyRewrite,
  onApplyPolished,
  onApplyAnchor,
  onRunAgent,
  onStop,
  onMessage,
}: AiFlavorOutputPanelProps) {
  const copyText = async (s: string, what: string) => {
    try {
      await navigator.clipboard.writeText(s);
      onMessage(`${what}已复制`);
    } catch {
      onMessage('复制失败:浏览器剪贴板不可用');
    }
  };

  const exportReport = async (format: 'md' | 'html') => {
    if (!report?.id) {
      onMessage('请先完成一次检测再导出');
      return;
    }
    try {
      const path = await ipc.aiflavorExportReport(report.id, format);
      onMessage(`已导出:${path}`);
    } catch (e) {
      onMessage(`导出失败: ${e}`);
    }
  };

  /** C4:逐句对照(原文 vs 改写后全文)。 */
  const diffPairs = useMemo(
    () =>
      extractedRewrite
        ? diffSentences(splitSentences(text), splitSentences(extractedRewrite))
        : [],
    [text, extractedRewrite],
  );

  /** 当前 pill 对应的智能体元数据 + 学术润色的可回填正文。 */
  const activeMeta = PILL_AGENTS.find((a) => a.id === activeAgent);
  const polished = extractPolished(agentOut);
  const disclosure = buildDisclosure(discModel, discUse);

  return (
    <section className="aif-col aif-right">
      <div className="aif-tabs">
        <button
          type="button"
          className={rightTab === 'rewrite' ? 'on' : ''}
          onClick={() => setRightTab('rewrite')}
        >
          {activeAgent === 'ai-flavor-rewriter' ? '改写对照' : '智能体输出'}
        </button>
        <button
          type="button"
          className={rightTab === 'academic' ? 'on' : ''}
          onClick={() => setRightTab('academic')}
        >
          学术披露
        </button>
      </div>

      {rightTab === 'rewrite' ? (
        activeAgent === 'ai-flavor-rewriter' ? (
          <div className="aif-card aif-rewrite">
          {!rewriteOut && !rewriting && (
            <div className="aif-empty">检测完成后点击「开始改写」,改写结果将在这里流式输出。</div>
          )}
          {rewriteOut && !rewriting && diffPairs.length > 0 && (
            <div className="aif-tabs sub">
              <button
                type="button"
                className={rewriteView === 'diff' ? 'on' : ''}
                onClick={() => setRewriteView('diff')}
              >
                逐句对照
              </button>
              <button
                type="button"
                className={rewriteView === 'raw' ? 'on' : ''}
                onClick={() => setRewriteView('raw')}
              >
                改写输出
              </button>
            </div>
          )}
          {rewriteOut && rewriteView === 'diff' && !rewriting ? (
            <div className="aif-diff">
              {diffPairs.map((p, i) => (
                <div key={i} className={`aif-diff-row ${p.kind}`}>
                  <span className="aif-diff-tag">
                    {p.kind === 'same' ? '＝' : p.kind === 'added' ? '＋' : '－'}
                  </span>
                  <div className="aif-diff-body">
                    {p.o && <div className="aif-diff-old">{p.o}</div>}
                    {p.r && <div className="aif-diff-new">{p.r}</div>}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            rewriteOut && (
              <div className="md">
                <MemoMarkdown content={rewriteOut} streaming={rewriting} withCodeBlock />
              </div>
            )
          )}
          {rewriteOut && !rewriting && (
            <div className="aif-row">
              <button
                type="button"
                className="btn ghost"
                disabled={!extractedRewrite}
                title={extractedRewrite ? '' : '未能从输出中提取「改写后全文」节'}
                onClick={onApplyRewrite}
              >
                应用回输入区
              </button>
              <button
                type="button"
                className="btn ghost"
                onClick={() => void copyText(rewriteOut, '改写结果')}
              >
                复制
              </button>
              <button
                type="button"
                className="btn ghost"
                disabled={!report?.id}
                title="导出当前检测报告(Markdown)"
                onClick={() => void exportReport('md')}
              >
                导出 MD
              </button>
              <button
                type="button"
                className="btn ghost"
                disabled={!report?.id}
                title="导出可打印 HTML(浏览器打印 → 存为 PDF)"
                onClick={() => void exportReport('html')}
              >
                导出 HTML
              </button>
            </div>
          )}
          {rewriting && (
            <button type="button" className="btn ghost" onClick={onStop}>
              停止
            </button>
          )}
          </div>
        ) : (
          <div className="aif-card aif-rewrite">
            <div className="aif-card-title">{activeMeta?.label}</div>
            <div className="aif-hint">{activeMeta?.desc}</div>
            {!agentOut && !agentRunning && (
              <div className="aif-empty">
                点击下方按钮运行该智能体，结果将在这里流式输出。
              </div>
            )}
            {agentOut && (
              <div className="md">
                <MemoMarkdown content={agentOut} streaming={agentRunning} withCodeBlock />
              </div>
            )}
            <div className="aif-row">
              <button
                type="button"
                className="btn primary"
                disabled={agentRunning}
                onClick={() => onRunAgent()}
              >
                {agentRunning ? '运行中…' : `运行${activeMeta?.label ?? '智能体'}`}
              </button>
              {agentOut && !agentRunning && activeAgent === 'academic-voice' && (
                <button
                  type="button"
                  className="btn ghost"
                  disabled={!polished}
                  title={polished ? '' : '未能从输出中提取「润色后文本」节'}
                  onClick={onApplyPolished}
                >
                  应用回输入区
                </button>
              )}
              {agentOut && !agentRunning && activeAgent === 'style-anchor' && (
                <button type="button" className="btn ghost" onClick={onApplyAnchor}>
                  写入风格锚点
                </button>
              )}
              {agentOut && !agentRunning && (
                <button
                  type="button"
                  className="btn ghost"
                  onClick={() => void copyText(agentOut, '智能体输出')}
                >
                  复制
                </button>
              )}
              {agentRunning && (
                <button type="button" className="btn ghost" onClick={onStop}>
                  停止
                </button>
              )}
            </div>
          </div>
        )
      ) : (
        <div className="aif-card">
          <div className="aif-card-title">AI 使用披露声明生成器</div>
          <div className="aif-hint">
            依据 Nature/Springer 政策:LLM 使用须在 Methods 或致谢中披露,AI 不可署名。
            本工具帮你生成合规草稿,不提供「规避检测」承诺。
          </div>
          <label className="lbl">模型名称</label>
          <input
            className="input"
            value={discModel}
            onChange={(e) => setDiscModel(e.target.value)}
            placeholder="如 GPT-5.2 / Claude Sonnet 4.5"
          />
          <label className="lbl" style={{ marginTop: 'var(--space-2)' }}>
            用途
          </label>
          <input
            className="input"
            value={discUse}
            onChange={(e) => setDiscUse(e.target.value)}
            placeholder="如 辅助语言润色 / 生成初稿框架"
          />
          <pre className="aif-disc">{disclosure}</pre>
          <button
            type="button"
            className="btn ghost"
            onClick={() => void copyText(disclosure, '披露声明')}
          >
            复制声明
          </button>
        </div>
      )}
    </section>
  );
}
