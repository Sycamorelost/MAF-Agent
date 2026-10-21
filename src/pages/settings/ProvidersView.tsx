//! 设置 · 模型服务商视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。
//! 状态与 IPC 处理仍在 SettingsPage,本组件只接收显式 props。

import * as ipc from '../../lib/ipc';
import { IconPlus, IconRefresh } from '../../components/icons';
import { ProviderSettingsCard } from './ProviderSettingsCard';

/** 新增服务商表单草稿(与 SettingsPage 中的 newTpl 同构)。 */
export type NewTemplateDraft = { display_name: string; base_url: string; modelsText: string };

type SlotPatch = { label?: string; enabled?: boolean; weight?: number; priority?: number };
type RotationPatch = {
  enabled?: boolean;
  cooldown_secs?: number;
  supports_stream_options?: boolean;
};

export interface ProvidersViewProps {
  templates: ipc.ProviderTemplate[];
  storedIds: string[];
  keyInputs: Record<string, string>;
  onKeyInput: (providerId: string, v: string) => void;
  savingKey: string | null;
  modelConfig: ipc.ProviderModelConfig;
  cards: ipc.ModelCard[];
  configMsg: string | null;
  tplMsg: string | null;
  fetching: Record<string, boolean>;
  fetchMsg: Record<string, string>;
  savingTpl: string | null;
  rotation: ipc.RotationConfigs;
  slotKeys: Record<string, ipc.KeySlotView[]>;
  addingTpl: boolean;
  onAddingTplChange: (v: boolean) => void;
  newTpl: NewTemplateDraft;
  onNewTplChange: (v: NewTemplateDraft) => void;
  onRestoreBuiltins: () => void;
  onClearKey: (providerId: string) => void;
  onPatchOverride: (providerId: string, model: string, patch: Partial<ipc.ModelOverride>) => void;
  onFetchModels: (providerId: string) => void;
  onEditTemplate: (providerId: string, patch: Partial<ipc.ProviderTemplate>) => void;
  onDeleteTemplate: (providerId: string, name: string) => void;
  onSaveKey: (providerId: string, label: string) => void;
  onPatchSlot: (providerId: string, slot: number, patch: SlotPatch) => void;
  onDeleteSlot: (providerId: string, slot: number) => void;
  onPatchRotation: (providerId: string, patch: RotationPatch) => void;
  reloadCards: () => void;
  onAddTemplate: () => void;
}

export function ProvidersView({
  templates,
  storedIds,
  keyInputs,
  onKeyInput,
  savingKey,
  modelConfig,
  cards,
  configMsg,
  tplMsg,
  fetching,
  fetchMsg,
  savingTpl,
  rotation,
  slotKeys,
  addingTpl,
  onAddingTplChange,
  newTpl,
  onNewTplChange,
  onRestoreBuiltins,
  onClearKey,
  onPatchOverride,
  onFetchModels,
  onEditTemplate,
  onDeleteTemplate,
  onSaveKey,
  onPatchSlot,
  onDeleteSlot,
  onPatchRotation,
  reloadCards,
  onAddTemplate,
}: ProvidersViewProps) {
  return (
    <>
      <h2>模型服务商</h2>
      <p className="set-desc">
        逐个服务商配置 API Key;「从厂家拉取」可获取最新模型列表(拉取成功后以厂家列表为准)。
        每个模型可单独启用/停用、改显示名与排序、设定默认生成参数、记忆注入策略与思考能力。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          服务商
          {configMsg || tplMsg ? (
            <span className="st">
              {configMsg}
              {configMsg && tplMsg ? ' · ' : ''}
              {tplMsg}
            </span>
          ) : null}
          <button
            className="btn-ghost"
            onClick={() => void onRestoreBuiltins()}
            title="仅补回被删除的内置模板,不影响已编辑/新增的模板"
          >
            <IconRefresh size={13} />
            恢复内置模板
          </button>
        </div>
        <div className="set-card-b">
          {templates.length === 0 && (
            <div className="set-hint">暂无服务商;可在下方「新增服务商」或「恢复内置模板」。</div>
          )}
          {templates.map((t) => (
            <ProviderSettingsCard
              key={t.id}
              tpl={t}
              hasKey={storedIds.includes(t.id)}
              keyInput={keyInputs[t.id] ?? ''}
              onKeyInput={(v) => onKeyInput(t.id, v)}
              onClearKey={() => void onClearKey(t.id)}
              savingKey={savingKey === t.id}
              overrides={modelConfig[t.id] ?? {}}
              cards={cards}
              onPatch={(model, patch) => void onPatchOverride(t.id, model, patch)}
              onFetch={() => void onFetchModels(t.id)}
              fetching={!!fetching[t.id]}
              fetchMsg={fetchMsg[t.id] ?? ''}
              savingTpl={savingTpl === t.id}
              onEdit={(patch) => void onEditTemplate(t.id, patch)}
              onDelete={() => void onDeleteTemplate(t.id, t.display_name)}
              rotation={rotation[t.id]}
              slots={slotKeys[t.id] ?? []}
              onAddKey={(label) => void onSaveKey(t.id, label)}
              onPatchSlot={(slot, patch) => void onPatchSlot(t.id, slot, patch)}
              onDeleteSlot={(slot) => void onDeleteSlot(t.id, slot)}
              onPatchRotation={(patch) => void onPatchRotation(t.id, patch)}
              onSaveCard={() => void reloadCards()}
            />
          ))}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          新增服务商
          <span className="set-hint" style={{ marginLeft: 'auto' }}>
            OpenAI 兼容协议
          </span>
        </div>
        <div className="set-card-b">
          {addingTpl ? (
            <>
              <div className="set-row">
                <label className="lbl">显示名称</label>
                <input
                  className="pg-field"
                  value={newTpl.display_name}
                  onChange={(e) => onNewTplChange({ ...newTpl, display_name: e.target.value })}
                  placeholder="如 我的中转站"
                />
              </div>
              <div className="set-row">
                <label className="lbl">Base URL</label>
                <input
                  className="pg-field"
                  value={newTpl.base_url}
                  onChange={(e) => onNewTplChange({ ...newTpl, base_url: e.target.value })}
                  placeholder="https://api.example.com/v1"
                />
              </div>
              <div className="set-row" style={{ alignItems: 'flex-start' }}>
                <label className="lbl" style={{ paddingTop: 6 }}>
                  模型列表
                </label>
                <textarea
                  className="pg-field"
                  rows={3}
                  value={newTpl.modelsText}
                  onChange={(e) => onNewTplChange({ ...newTpl, modelsText: e.target.value })}
                  placeholder={'model-a\nmodel-b'}
                  style={{ resize: 'vertical' }}
                />
              </div>
              <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
                <button className="btn-ghost" onClick={() => onAddingTplChange(false)}>
                  取消
                </button>
                <span className="sp" />
                <button
                  className="btn-primary"
                  onClick={() => void onAddTemplate()}
                  disabled={savingTpl === '__new__'}
                >
                  创建服务商
                </button>
              </div>
            </>
          ) : (
            <div className="set-actions">
              <span className="set-hint">
                可添加任意 OpenAI 兼容服务商;配置 Key 后即可在对话中选用。
              </span>
              <span className="sp" />
              <button className="btn-primary" onClick={() => onAddingTplChange(true)}>
                <IconPlus size={14} />
                新增服务商
              </button>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
