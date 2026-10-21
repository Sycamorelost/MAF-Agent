import { useNavigate } from 'react-router-dom';
import * as ipc from '../../../lib/ipc';
import { IconDoc, IconGlobe, IconPlay } from '../../../components/icons';
import { estimateStagedTokens, estimateTokens, fmtTok } from '../helpers';

/** 面板底部:引擎/错误提示、预计消耗与启动按钮。 */
export function ConfigFooter({
  topic,
  params,
  blueprintName,
  starting,
  error,
  engineReady,
  whitelistDomains,
  onNaming,
  onStart,
}: {
  topic: string;
  params: ipc.ResearchParams;
  blueprintName: string | null;
  starting: boolean;
  error: string | null;
  engineReady: boolean;
  whitelistDomains: string[];
  onNaming: () => void;
  onStart: () => void | Promise<void>;
}) {
  const nav = useNavigate();
  const est = params.mode === 'staged' ? estimateStagedTokens(params) : estimateTokens(params);

  return (
    <>
      {!engineReady && (
        <div className="pg-warn">
          尚未配置可用的搜索引擎,请先到「设置 → 联网搜索」完成配置。
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-ghost" onClick={() => nav('/settings')}>
              <IconGlobe size={14} />
              前往设置
            </button>
          </div>
        </div>
      )}
      {error && <div className="pg-warn">{error}</div>}

      <div className="pg-hint" style={{ color: 'var(--warn)' }}>
        研究任务消耗 token 较多,请留意预算设置。
      </div>

      <div className="pg-foot">
        <div className="pg-est">
          <span>方案</span>
          <b className="ellipsis" title={blueprintName ?? undefined}>
            {blueprintName ?? '自定义配置'}
          </b>
        </div>
        <div className="pg-est">
          <span>预计消耗</span>
          <b>
            {fmtTok(est)}
            {params.token_budget > 0 &&
            (params.mode === 'staged'
              ? estimateStagedTokens({ ...params, token_budget: 0 })
              : estimateTokens({ ...params, token_budget: 0 })) > params.token_budget
              ? ' · 触顶预算'
              : ''}
          </b>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-est">
            <span>阶段</span>
            <b>
              {params.stages.filter((s) => s.enabled).length} / {params.stages.length} 启用
            </b>
          </div>
        )}
        <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
          <button
            className="btn-ghost"
            style={{ flex: 1, justifyContent: 'center', border: '1px solid var(--border)' }}
            onClick={onNaming}
            title="将当前配置保存为方案"
          >
            <IconDoc size={13} />
            存为方案
          </button>
          <button
            className="pg-run"
            style={{ flex: 2 }}
            onClick={onStart}
            disabled={
              !topic.trim() ||
              !engineReady ||
              starting ||
              (params.source_mode === 'whitelist' && whitelistDomains.length === 0) ||
              (params.mode === 'staged' && params.stages.every((s) => !s.enabled))
            }
          >
            <IconPlay size={15} />
            {starting
              ? '启动中…'
              : params.mode === 'staged'
                ? '开始专项研究'
                : params.mode === 'concise'
                  ? '开始简洁研究'
                  : '开始研究'}
          </button>
        </div>
      </div>
    </>
  );
}
