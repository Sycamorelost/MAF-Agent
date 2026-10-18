import { useEffect, useState } from 'react';
import { useProvidersStore } from '../stores/providers';
import * as ipc from '../lib/ipc';
import { IconRefresh } from './icons';

interface Props {
  onClose: () => void;
  /** 选定模型(providerId, model):由父组件显式处理切换与记忆交接 */
  onPick: (providerId: string, model: string) => void;
}

/** 提炼专用小模型设置(D3.7):缺省 = 跟随当前会话模型。 */
function SummarizeModelSection() {
  const { templates, hasKey, modelConfig } = useProvidersStore();
  const [providerId, setProviderId] = useState('');
  const [model, setModel] = useState('');
  const [saved, setSaved] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [fullSettings, setFullSettings] = useState<ipc.AppSettings | null>(null);

  useEffect(() => {
    ipc.getSettings().then((s) => {
      setFullSettings(s);
      setProviderId(s.summarize_provider_id ?? '');
      setModel(s.summarize_model ?? '');
      setLoaded(true);
    });
  }, []);

  const readyTemplates = templates.filter((t) => hasKey[t.id] && t.models.length > 0);
  const tpl = readyTemplates.find((t) => t.id === providerId);

  const onSave = async (pid: string, m: string) => {
    if (!fullSettings) return;
    await ipc.saveSettings({
      ...fullSettings,
      summarize_provider_id: pid || null,
      summarize_model: pid ? m || null : null,
    });
    setSaved(true);
  };

  if (!loaded) return null;

  return (
    <div className="summarize-section">
      <div className="summarize-title">记忆提炼模型(可选,缺省使用当前会话模型)</div>
      <div className="summarize-row">
        <select
          className="key-input"
          value={providerId}
          onChange={(e) => {
            const pid = e.target.value;
            setProviderId(pid);
            const t = readyTemplates.find((x) => x.id === pid);
            const m = t?.models[0] ?? '';
            setModel(m);
            setSaved(false);
          }}
        >
          <option value="">跟随当前会话模型</option>
          {readyTemplates.map((t) => (
            <option key={t.id} value={t.id}>
              {t.display_name}
            </option>
          ))}
        </select>
        {providerId && tpl && (
          <select
            className="key-input"
            value={model}
            onChange={(e) => {
              setModel(e.target.value);
              setSaved(false);
            }}
          >
            {tpl.models.map((m) => (
              <option key={m} value={m}>
                {modelConfig[tpl.id]?.[m]?.display_name?.trim() || m}
              </option>
            ))}
          </select>
        )}
        <button
          type="button"
          className="btn primary"
          onClick={() => onSave(providerId, model)}
        >
          {saved ? '已保存' : '保存'}
        </button>
      </div>
    </div>
  );
}

export default function ModelSwitchDialog({ onClose, onPick }: Props) {
  const { templates, hasKey, current, fetchModels, modelConfig, loadModelConfig } =
    useProvidersStore();
  /** 只展示「已配置 Key 且有可用模型」的服务商:可直接使用的模型一目了然 */
  const readyTemplates = templates.filter((t) => hasKey[t.id] && t.models.length > 0);
  /** provider_id -> 拉取中 / 结果提示 */
  const [fetching, setFetching] = useState<Record<string, boolean>>({});
  const [fetchMsg, setFetchMsg] = useState<Record<string, string>>({});
  /** 模型卡(D7.1):按 provider + official_model_id 匹配展示元信息 */
  const [cards, setCards] = useState<ipc.ModelCard[]>([]);

  useEffect(() => {
    ipc.listModelCards().then(setCards).catch(() => {});
    loadModelConfig().catch(() => {});
  }, [loadModelConfig]);

  const matchCard = (providerId: string, model: string) =>
    cards.find((c) => c.provider === providerId && c.official_model_id === model);

  /** 显示名优先级:用户覆盖 > 模型卡 > 官方 id。 */
  const labelOf = (providerId: string, model: string) =>
    modelConfig[providerId]?.[model]?.display_name?.trim() || matchCard(providerId, model)?.display_name || model;

  const onFetch = async (providerId: string) => {
    setFetching((s) => ({ ...s, [providerId]: true }));
    setFetchMsg((s) => ({ ...s, [providerId]: '' }));
    try {
      const models = await fetchModels(providerId);
      setFetchMsg((s) => ({ ...s, [providerId]: `已拉取 ${models.length} 个模型` }));
    } catch (e) {
      setFetchMsg((s) => ({
        ...s,
        [providerId]: `拉取失败:${e instanceof Error ? e.message : String(e)}`,
      }));
    } finally {
      setFetching((s) => ({ ...s, [providerId]: false }));
    }
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">切换模型</div>
        <div className="dialog-body">
          {readyTemplates.length === 0 && (
            <div className="switch-empty">
              暂无可用模型:请先到「设置 → 模型服务商」配置 API Key 并启用模型。
            </div>
          )}
          {readyTemplates.map((t) => (
            <div className="provider-block" key={t.id}>
              <div className="provider-head">
                <span className="name">{t.display_name}</span>
                <span className="badge">
                  {t.models.length} 个模型
                </span>
                <button
                  type="button"
                  className="tool-chip"
                  style={{ marginLeft: 'auto' }}
                  disabled={fetching[t.id]}
                  onClick={() => onFetch(t.id)}
                  title="通过 GET /models 从厂家拉取可用模型列表"
                >
                  <IconRefresh size={12} />
                  <span>{fetching[t.id] ? '拉取中…' : '从厂家拉取'}</span>
                </button>
              </div>
              {fetchMsg[t.id] && (
                <div
                  style={{
                    padding: '0 var(--pad-x) var(--space-2)',
                    fontSize: 'var(--font-caption)',
                    color: fetchMsg[t.id].startsWith('拉取失败') ? 'var(--err)' : 'var(--text-3)',
                  }}
                >
                  {fetchMsg[t.id]}
                </div>
              )}
              <div style={{ padding: '0 var(--pad-x) var(--space-3)' }}>
                <div className="provider-models" style={{ padding: 0 }}>
                  {t.models.map((m) => {
                    const card = matchCard(t.id, m);
                    return (
                      <div className="model-chip-wrap" key={m}>
                        <button
                          className={`model-chip ${
                            current?.providerId === t.id && current?.model === m ? 'active' : ''
                          }`}
                          onClick={() => {
                            onPick(t.id, m);
                            onClose();
                          }}
                        >
                          {labelOf(t.id, m)}
                        </button>
                        {card && (
                          <div
                            className="model-chip-meta"
                            title={`${card.official_model_id}\n上下文 ${ipc.formatTokens(card.context_window)} · 输出 ${ipc.formatTokens(card.max_output)}\n${card.tags.join(' / ')}`}
                          >
                            <span>{ipc.formatTokens(card.context_window)} ctx</span>
                            <span className={`cost-badge tier-${card.cost_tier}`}>
                              {ipc.COST_TIER_LABEL[card.cost_tier] ?? card.cost_tier}
                            </span>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ))}
          <SummarizeModelSection />
        </div>
      </div>
    </div>
  );
}
