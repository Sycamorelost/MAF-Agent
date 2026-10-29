//! 设置 · 记忆与提炼视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import * as ipc from '../../lib/ipc';
import { IconCheck } from '../../components/icons';

export interface MemoryViewProps {
  settings: ipc.AppSettings | null;
  /** 已配置 Key 的服务商(仅这些可用于提炼) */
  keyedTemplates: ipc.ProviderTemplate[];
  settingsSaved: boolean;
  savingSettings: boolean;
  onPatchSettings: (patch: Partial<ipc.AppSettings>) => void;
  onSaveSettings: () => void;
}

export function MemoryView({
  settings,
  keyedTemplates,
  settingsSaved,
  savingSettings,
  onPatchSettings,
  onSaveSettings,
}: MemoryViewProps) {
  return (
    <>
      <h2>记忆与提炼</h2>
      <p className="set-desc">对话后自动触发提炼;留空则跟随当前会话模型。</p>

      <div className="set-card">
        <div className="set-card-h">
          提炼模型
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">提炼 Provider</label>
            <select
              className="pg-field"
              value={settings?.summarize_provider_id ?? ''}
              onChange={(e) =>
                onPatchSettings({ summarize_provider_id: e.target.value || null })
              }
            >
              <option value="">跟随当前会话模型</option>
              {keyedTemplates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.display_name}
                </option>
              ))}
            </select>
          </div>
          <div className="set-row">
            <label className="lbl">提炼模型</label>
            {(() => {
              const pid = settings?.summarize_provider_id ?? '';
              const tpl = keyedTemplates.find((t) => t.id === pid);
              const models = tpl?.models ?? [];
              const cur = settings?.summarize_model ?? '';
              // 已保存的值不在当前可选列表(模型被停用/Provider 已换)时保留显示,避免静默丢失
              const opts = cur && !models.includes(cur) ? [cur, ...models] : models;
              return (
                <select
                  className="pg-field"
                  value={cur}
                  disabled={!pid}
                  onChange={(e) =>
                    onPatchSettings({ summarize_model: e.target.value || null })
                  }
                >
                  <option value="">{pid ? '跟随 Provider 默认模型' : '跟随当前会话模型'}</option>
                  {opts.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              );
            })()}
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSettings} disabled={savingSettings}>
              保存记忆设置
            </button>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">上下文与性能</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">每轮携带的历史消息数</label>
            <select
              className="pg-field"
              value={String(settings?.context_window_messages ?? 24)}
              onChange={(e) =>
                onPatchSettings({ context_window_messages: Number(e.target.value) })
              }
            >
              <option value="8">8 条(约 4 轮,最快)</option>
              <option value="16">16 条(约 8 轮)</option>
              <option value="24">24 条(约 12 轮,推荐)</option>
              <option value="40">40 条(约 20 轮)</option>
              <option value="0">全部历史(最慢,长会话慎用)</option>
            </select>
          </div>
          <p className="set-desc" style={{ margin: 'var(--space-2) 0 0' }}>
            每次请求发给模型的历史消息条数(非模型 token 上限)。条数越多,长会话的 token
            消耗与首字延迟越高;超出窗口的旧内容仍会通过记忆提炼摘要保留。
          </p>
        </div>
      </div>
    </>
  );
}
