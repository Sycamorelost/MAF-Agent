import { useState } from 'react';
import * as ipc from '../../lib/ipc';
import { IconChevronDown, IconPlus, IconRefresh, IconTrash } from '../../components/icons';
import { ModelConfigCard } from './ModelConfigCard';

export function ProviderSettingsCard({
  tpl,
  hasKey,
  keyInput,
  onKeyInput,
  onClearKey,
  savingKey,
  overrides,
  cards,
  onPatch,
  onFetch,
  fetching,
  fetchMsg,
  savingTpl,
  onEdit,
  onDelete,
  rotation,
  slots,
  onAddKey,
  onPatchSlot,
  onDeleteSlot,
  onPatchRotation,
  onSaveCard,
}: {
  tpl: ipc.ProviderTemplate;
  hasKey: boolean;
  keyInput: string;
  onKeyInput: (v: string) => void;
  onClearKey: () => void;
  savingKey: boolean;
  overrides: Record<string, ipc.ModelOverride>;
  cards: ipc.ModelCard[];
  onPatch: (model: string, patch: Partial<ipc.ModelOverride>) => void;
  onFetch: () => void;
  fetching: boolean;
  fetchMsg: string;
  savingTpl: boolean;
  onEdit: (patch: Partial<ipc.ProviderTemplate>) => void;
  onDelete: () => void;
  /** 该 Provider 的轮换配置(未配置时为 undefined) */
  rotation?: ipc.RotationConfig;
  /** 已配置的 Key 槽位 */
  slots: ipc.KeySlotView[];
  onAddKey: (label: string) => void;
  onPatchSlot: (
    slot: number,
    patch: { label?: string; enabled?: boolean; weight?: number; priority?: number },
  ) => void;
  onDeleteSlot: (slot: number) => void;
  onPatchRotation: (patch: {
    enabled?: boolean;
    cooldown_secs?: number;
    supports_stream_options?: boolean;
  }) => void;
  /** 模型卡价格/额度保存后,刷新卡片列表 */
  onSaveCard: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [newModel, setNewModel] = useState('');
  const [newLabel, setNewLabel] = useState('');
  const models = tpl.all_models.length > 0 ? tpl.all_models : tpl.models;
  const rot = rotation ?? {
    enabled: false,
    strategy: 'round_robin',
    cooldown_secs: 60,
    supports_stream_options: true,
    keys: [],
  };

  const addModel = () => {
    const m = newModel.trim();
    if (!m) return;
    if (!models.includes(m)) onEdit({ models: [...models, m] });
    setNewModel('');
  };

  return (
    <div className={`prov-card${open ? ' open' : ''}`}>
      <div className="prov-card-h" onClick={() => setOpen((v) => !v)}>
        <IconChevronDown size={15} className="arr" />
        <span className="nm">{tpl.display_name}</span>
        <span className={`settings-badge ${hasKey ? 'ok' : ''}`}>
          {hasKey ? '已配置 Key' : '未配置'}
        </span>
        <span className="cnt">
          {tpl.models.length} / {models.length} 个模型
        </span>
        <span className="url">{tpl.base_url}</span>
        <button
          className="prov-del"
          title="删除该服务商(含内置模板)"
          disabled={savingTpl}
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
        >
          <IconTrash size={14} />
        </button>
      </div>
      <div className="prov-card-b">
        <div className="set-row">
          <div className="set-fld">
            <label className="lbl">显示名</label>
            <input
              className="pg-field"
              defaultValue={tpl.display_name}
              spellCheck={false}
              disabled={savingTpl}
              onBlur={(e) => {
                const v = e.target.value.trim();
                if (v && v !== tpl.display_name) onEdit({ display_name: v });
                else e.target.value = tpl.display_name;
              }}
            />
          </div>
          <div className="set-fld">
            <label className="lbl">Base URL</label>
            <input
              className="pg-field"
              defaultValue={tpl.base_url}
              placeholder="https://api.example.com/v1"
              spellCheck={false}
              disabled={savingTpl}
              onBlur={(e) => {
                const v = e.target.value.trim();
                if (v !== tpl.base_url) onEdit({ base_url: v });
              }}
            />
          </div>
        </div>
        <div className="set-prov-line">
          <input
            className="pg-field"
            type="password"
            placeholder={hasKey ? '输入新 Key(追加为新槽位)' : '输入 API Key'}
            value={keyInput}
            onChange={(e) => onKeyInput(e.target.value)}
          />
          <input
            className="pg-field"
            style={{ maxWidth: 140 }}
            placeholder="备注(可选)"
            value={newLabel}
            spellCheck={false}
            onChange={(e) => setNewLabel(e.target.value)}
          />
          <button
            className="btn-primary"
            onClick={() => {
              onAddKey(newLabel);
              setNewLabel('');
            }}
            disabled={!keyInput || savingKey}
          >
            添加
          </button>
          {tpl.openai_compat && (
            <button className="btn-ghost" onClick={onFetch} disabled={!hasKey || fetching}>
              <IconRefresh size={13} />
              {fetching ? '拉取中…' : '从厂家拉取'}
            </button>
          )}
          {hasKey && (
            <button className="btn-ghost" onClick={onClearKey}>
              清除全部
            </button>
          )}
        </div>
        {slots.length > 0 && (
          <div className="key-slots">
            <div className="key-slots-h">
              多 Key 轮换
              <button
                className={`mcard-toggle${rot.enabled ? ' on' : ''}`}
                onClick={() => onPatchRotation({ enabled: !rot.enabled })}
              >
                <span className="dot" />
                {rot.enabled ? '已启用' : '已停用'}
              </button>
              <label className="cool-num">
                限流冷却
                <input
                  className="pg-field num"
                  type="number"
                  min={1}
                  max={3600}
                  defaultValue={rot.cooldown_secs}
                  onBlur={(e) => {
                    const v = parseInt(e.target.value, 10);
                    if (Number.isFinite(v)) {
                      onPatchRotation({ cooldown_secs: Math.min(3600, Math.max(1, v)) });
                    }
                  }}
                />
                秒
              </label>
              <span className="sp" />
              <button
                className={`mcard-toggle${rot.supports_stream_options ? ' on' : ''}`}
                title="中转站不认 stream_options 时关闭(关闭后无法采集精确 Token 用量)"
                onClick={() =>
                  onPatchRotation({ supports_stream_options: !rot.supports_stream_options })
                }
              >
                <span className="dot" />
                兼容 stream_options
              </button>
            </div>
            {slots.map((s) => (
              <div className="key-slot" key={s.slot}>
                <span className="idx">#{s.slot}</span>
                <input
                  className="pg-field"
                  defaultValue={s.label}
                  placeholder="备注"
                  spellCheck={false}
                  onBlur={(e) => {
                    const v = e.target.value.trim();
                    if (v !== s.label) onPatchSlot(s.slot, { label: v });
                  }}
                />
                <label className="slot-num" title="权重:相对流量份额,越大承担越多请求">
                  权重
                  <input
                    className="pg-field num"
                    type="number"
                    min={1}
                    max={50}
                    defaultValue={s.weight}
                    onBlur={(e) => {
                      const v = parseInt(e.target.value, 10);
                      if (Number.isFinite(v) && v !== s.weight) {
                        onPatchSlot(s.slot, { weight: Math.min(50, Math.max(1, v)) });
                      }
                    }}
                  />
                </label>
                <label className="slot-num" title="优先级:越大越先使用,高优先级全冷却时才降级">
                  优先级
                  <input
                    className="pg-field num"
                    type="number"
                    min={0}
                    max={99}
                    defaultValue={s.priority}
                    onBlur={(e) => {
                      const v = parseInt(e.target.value, 10);
                      if (Number.isFinite(v) && v !== s.priority) {
                        onPatchSlot(s.slot, { priority: Math.min(99, Math.max(0, v)) });
                      }
                    }}
                  />
                </label>
                <code className="key-mask">{s.masked ?? '—'}</code>
                {s.cooldown_remaining > 0 && <span className="cool">冷却 {s.cooldown_remaining}s</span>}
                <button
                  className={`mcard-toggle${s.enabled ? ' on' : ''}`}
                  onClick={() => onPatchSlot(s.slot, { enabled: !s.enabled })}
                >
                  <span className="dot" />
                  {s.enabled ? '启用' : '停用'}
                </button>
                <button className="prov-del" title="删除该 Key" onClick={() => onDeleteSlot(s.slot)}>
                  <IconTrash size={13} />
                </button>
              </div>
            ))}
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              仅在初始请求被限流/报错且尚未输出内容时切换到下一把 Key;已开始输出则保持当前 Key。
              权重影响流量份额,优先级高的 Key 优先使用。
              若另一个 Provider 的 base_url 与本服务商一致、且同样启用了轮换,其 Key 会自动并入同一编组共享轮换。
            </div>
          </div>
        )}
        {fetchMsg && (
          <div
            className="set-hint"
            style={{
              marginTop: 'var(--space-2)',
              color: fetchMsg.startsWith('拉取失败') ? 'var(--err)' : undefined,
            }}
          >
            {fetchMsg}
          </div>
        )}
        {models.length > 0 ? (
          <div className="mcard-list">
            {models.map((m) => (
              <ModelConfigCard
                key={m}
                providerId={tpl.id}
                model={m}
                override={overrides[m]}
                card={cards.find((c) => c.provider === tpl.id && c.official_model_id === m)}
                onPatch={onPatch}
                onSaveCard={onSaveCard}
              />
            ))}
          </div>
        ) : (
          <div className="set-hint" style={{ marginTop: 'var(--space-3)' }}>
            暂无模型{hasKey ? ',可「从厂家拉取」获取' : ';配置 Key 后可「从厂家拉取」'}。
          </div>
        )}
        <div className="set-prov-line">
          <input
            className="pg-field"
            placeholder="手动添加模型 id,回车确认"
            value={newModel}
            spellCheck={false}
            disabled={savingTpl}
            onChange={(e) => setNewModel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') addModel();
            }}
          />
          <button className="btn-ghost" onClick={addModel} disabled={!newModel.trim() || savingTpl}>
            <IconPlus size={13} />
            添加模型
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---------------- 关于页:myAgent 使用条款(v1.9.0,对标 Microsoft 使用条款组织逻辑) ---------------- */

/** 条款编号中文序号(一~十五) */





