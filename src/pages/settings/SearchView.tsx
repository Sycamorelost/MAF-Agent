//! 设置 · 联网搜索视图。
//! v2:小白友好 —— 默认「一键搜」零 Key 即用,复杂选项折叠。

import * as ipc from '../../lib/ipc';
import { ENGINES } from './constants';

export type SearchDraft = {
  mode: ipc.SearchMode;
  engine: string;
  base_url: string;
  api_key: string;
  mcp_server: string;
  mcp_tool: string;
};

export interface SearchViewProps {
  searchCfg: ipc.SearchConfigDto | null;
  searchDraft: SearchDraft;
  engineLabel: string;
  engineReady: boolean;
  savingSearch: boolean;
  searchTestStatus: string | null;
  searchTesting: boolean;
  /** MCP 自定义模式下拉候选(预置 + 用户自己加的) */
  mcpServerNames: string[];
  onDraftChange: (patch: Partial<SearchDraft>) => void;
  onSaveSearch: () => void;
  onTestSearch: () => void;
}

export function SearchView({
  searchCfg,
  searchDraft,
  engineLabel,
  engineReady,
  savingSearch,
  searchTestStatus,
  searchTesting,
  mcpServerNames,
  onDraftChange,
  onSaveSearch,
  onTestSearch,
}: SearchViewProps) {
  const isMcp = searchDraft.mode === 'mcp';
  // 当前 MCP server 是否需要 Key(后端返回,mcp 模式下才有意义)
  const needsKey = isMcp && !!searchCfg?.mcp_needs_key;

  return (
    <>
      <h2>联网搜索</h2>
      <p className="set-desc">
        对话里的联网检索从这里出去,默认即用无需配置;也可切换成自建引擎或第三方 API。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          搜索引擎配置
          <span className={`st${engineReady ? '' : ' warn'}`}>
            {engineReady ? '已就绪' : '未就绪'} · {engineLabel}
          </span>
        </div>
        <div className="set-card-b">
          {/* ============ 小白区:一行选方式 ============ */}
          <div className="set-row">
            <label className="lbl">搜索来源</label>
            <select
              className="pg-field"
              value={searchDraft.mode}
              onChange={(e) => onDraftChange({ mode: e.target.value as ipc.SearchMode })}
            >
              <option value="mcp">一键搜(推荐,零 Key)</option>
              <option value="api">API 直连(需自备 API Key)</option>
            </select>
          </div>

          {isMcp ? (
            <>
              {/* 小白区:零 Key 直接用 */}
              <div className="set-hint" style={{ marginTop: 'var(--space-1)', color: 'var(--text-dim)' }}>
                默认走 9 个免费引擎并行(DuckDuckGo / 搜狗 / 百度 / Bing 等),无需 Key;
                如需更快结果或更高配额可在下方展开付费档。
              </div>

              {/* 付费档 Key(仅当当前 server 需要 Key 时显示) */}
              {needsKey && (
                <div className="set-row" style={{ marginTop: 'var(--space-3)' }}>
                  <label className="lbl">API Key(付费档)</label>
                  <input
                    className="pg-field"
                    type="password"
                    value={searchDraft.api_key}
                    onChange={(e) => onDraftChange({ api_key: e.target.value })}
                    placeholder={
                      searchCfg?.has_api_key
                        ? '已配置;留空则保留'
                        : '当前 MCP server 需要 API Key,在此填写'
                    }
                  />
                </div>
              )}

              {/* 高级区:自定义 MCP 服务(折叠,用 presetOnly 控制展开) */}
              <details className="set-details" style={{ marginTop: 'var(--space-2)' }}>
                <summary style={{ cursor: 'pointer', color: 'var(--text-dim)', fontSize: 12 }}>
                  高级:自定义 MCP 搜索服务
                </summary>
                <div className="set-stack" style={{ marginTop: 'var(--space-2)' }}>
                  <div className="set-row">
                    <label className="lbl">MCP 服务</label>
                    <select
                      className="pg-field"
                      value={searchDraft.mcp_server}
                      onChange={(e) => onDraftChange({ mcp_server: e.target.value, mcp_tool: '' })}
                    >
                      <option value="">默认(预置「联网搜索」)</option>
                      {mcpServerNames
                        .filter((n) => n !== '联网搜索')
                        .map((n) => (
                          <option key={n} value={n}>
                            {n}
                          </option>
                        ))}
                    </select>
                  </div>
                  <div className="set-row">
                    <label className="lbl">工具名</label>
                    <select
                      className="pg-field"
                      value={searchDraft.mcp_tool}
                      onChange={(e) => onDraftChange({ mcp_tool: e.target.value })}
                    >
                      <option value="">自动(挑名字含 search 的工具)</option>
                      {(searchCfg?.mcp_tool_names ?? []).map((t) => (
                        <option key={t} value={t}>
                          {t}
                        </option>
                      ))}
                    </select>
                  </div>
                  {/* 已启动 server 的工具列表预览 */}
                  {searchCfg && searchCfg.mcp_tool_names.length > 0 && (
                    <div className="set-hint" style={{ color: 'var(--text-dim)', fontSize: 12 }}>
                      已启动,工具:{searchCfg.mcp_tool_names.join('、')}
                    </div>
                  )}
                </div>
              </details>
            </>
          ) : (
            <>
              <div className="set-row" style={{ marginTop: 'var(--space-1)' }}>
                <label className="lbl">搜索引擎</label>
                <select
                  className="pg-field"
                  value={searchDraft.engine}
                  onChange={(e) => onDraftChange({ engine: e.target.value })}
                >
                  {ENGINES.map((e) => (
                    <option key={e.id} value={e.id}>
                      {e.name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="set-row">
                <label className="lbl">Base URL</label>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <input
                    className="pg-field"
                    value={searchDraft.base_url}
                    onChange={(e) => onDraftChange({ base_url: e.target.value })}
                    placeholder="SearXNG 需填写实例地址;其余引擎可留空"
                  />
                </div>
              </div>
              <div className="set-row">
                <label className="lbl">API Key</label>
                <input
                  className="pg-field"
                  type="password"
                  value={searchDraft.api_key}
                  onChange={(e) => onDraftChange({ api_key: e.target.value })}
                  placeholder={
                    searchCfg?.has_api_key
                      ? '留空则保留已配置的 Key'
                      : searchDraft.engine === 'deepseek'
                        ? '留空则自动复用模型服务商中的 DeepSeek Key'
                        : 'SearXNG 可留空'
                  }
                />
              </div>
            </>
          )}

          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button className="btn-ghost" onClick={onTestSearch} disabled={searchTesting}>
              {searchTesting ? '测试中…' : '测试搜索'}
            </button>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSearch} disabled={savingSearch}>
              保存搜索设置
            </button>
          </div>
          {searchTestStatus && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              {searchTestStatus}
            </div>
          )}
          {!searchTestStatus && !searchTesting && (
            <div className="set-hint" style={{ marginTop: 'var(--space-1)', color: 'var(--text-dim)', fontSize: 12 }}>
              「测试搜索」按当前选择立即执行(无需先保存),以便确认切换是否真的生效。
            </div>
          )}
        </div>
      </div>
    </>
  );
}
