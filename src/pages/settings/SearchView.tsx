//! 设置 · 联网搜索视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import * as ipc from '../../lib/ipc';
import { ENGINES } from './constants';

export type SearchDraft = { engine: string; base_url: string; api_key: string };

export interface SearchViewProps {
  searchCfg: ipc.SearchConfigDto | null;
  searchDraft: SearchDraft;
  engineLabel: string;
  engineReady: boolean;
  savingSearch: boolean;
  searchTestStatus: string | null;
  searchTesting: boolean;
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
  onDraftChange,
  onSaveSearch,
  onTestSearch,
}: SearchViewProps) {
  return (
    <>
      <h2>联网搜索</h2>
      <p className="set-desc">深度研究与对话联网的检索来源,国内渠道优先。</p>

      <div className="set-card">
        <div className="set-card-h">
          搜索引擎配置
          <span className={`st${engineReady ? '' : ' warn'}`}>
            {engineReady ? '已就绪' : '未就绪'} · {engineLabel}
          </span>
        </div>
        <div className="set-card-b">
          <div className="set-row">
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
        </div>
      </div>
    </>
  );
}
