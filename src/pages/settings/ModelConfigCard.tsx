import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import { IconChevronDown, IconEdit } from '../../components/icons';
import { CardPriceEditor } from './CardPriceEditor';
import { capToValue, numOrNull, MEMORY_STRATEGY_OPTIONS, THINK_CAP_OPTIONS } from './constants';

export function ModelConfigCard({
  providerId,
  model,
  override,
  card,
  onPatch,
  onSaveCard,
}: {
  providerId: string;
  model: string;
  override: ipc.ModelOverride | undefined;
  card: ipc.ModelCard | undefined;
  onPatch: (model: string, patch: Partial<ipc.ModelOverride>) => void;
  onSaveCard: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [editingPrice, setEditingPrice] = useState(false);
  // 思考能力手动修正回显(null = 未加载;'auto' = 未修正,按规则表推断)
  const [capSel, setCapSel] = useState<string | null>(null);
  const ov = override ?? ipc.EMPTY_MODEL_OVERRIDE;
  const enabled = ov.enabled !== false;
  const alias = ov.display_name?.trim() || '';
  const name = alias || card?.display_name || model;
  const p = card?.pricing;
  const inPrice = p?.input ?? p?.input_cache_miss ?? p?.input_cache_hit ?? null;
  const hasPerPrice = p?.per_image != null || p?.per_char != null;
  const hasTokenPrice = inPrice != null && p?.output != null;
  const hasAnyPrice = hasPerPrice || hasTokenPrice;

  // 展开时加载手动修正原始值(与解析后能力区分,能准确显示「自动/手动」)
  useEffect(() => {
    if (!open || capSel !== null) return;
    let alive = true;
    ipc
      .getReasoningOverride(providerId, model)
      .then((c) => alive && setCapSel(capToValue(c)))
      .catch(() => alive && setCapSel('auto'));
    return () => {
      alive = false;
    };
  }, [open, capSel, providerId, model]);

  /** 应用手动修正:乐观更新,失败回退。 */
  const applyCap = async (v: string) => {
    const prev = capSel ?? 'auto';
    setCapSel(v);
    try {
      await ipc.saveReasoningOverride(
        providerId,
        model,
        THINK_CAP_OPTIONS.find((o) => o.value === v)?.cap ?? null,
      );
    } catch {
      setCapSel(prev);
    }
  };

  return (
    <div className={`mcard${open ? ' open' : ''}${enabled ? '' : ' off'}`}>
      <div className="mcard-h" onClick={() => setOpen((v) => !v)}>
        <IconChevronDown size={14} className="arr" />
        <span className="nm">{name}</span>
        {alias && <span className="mid">{model}</span>}
        <span className="sp" />
        {card && <span className="mid">{ipc.formatTokens(card.context_window)} ctx</span>}
        {card && (
          <span className={`cost-badge tier-${card.cost_tier}`}>
            {ipc.COST_TIER_LABEL[card.cost_tier] ?? card.cost_tier}
          </span>
        )}
        <span
          className={`mcard-toggle${enabled ? ' on' : ''}`}
          title={enabled ? '停用后不出现在切换模型列表' : '重新启用'}
          onClick={(e) => {
            e.stopPropagation();
            onPatch(model, { enabled: !enabled });
          }}
        >
          <span className="dot" />
          {enabled ? '启用' : '停用'}
        </span>
      </div>
      <div className="mcard-b">
        <div className="set-row">
          <div className="set-fld">
            <label className="lbl">显示名</label>
            <input
              className="pg-field"
              defaultValue={ov.display_name ?? ''}
              placeholder={card?.display_name ?? model}
              spellCheck={false}
              onBlur={(e) => onPatch(model, { display_name: e.target.value.trim() || null })}
            />
          </div>
          <div className="set-fld">
            <label className="lbl">排序</label>
            <input
              className="pg-field num"
              type="number"
              defaultValue={ov.order ?? ''}
              placeholder="默认"
              onBlur={(e) => onPatch(model, { order: numOrNull(e.target.value) })}
            />
          </div>
        </div>
        <div className="set-row">
          <div className="set-fld">
            <label className="lbl">temperature</label>
            <input
              className="pg-field num"
              type="number"
              step="0.1"
              min="0"
              max="2"
              defaultValue={ov.temperature ?? ''}
              placeholder="默认"
              onBlur={(e) => onPatch(model, { temperature: numOrNull(e.target.value) })}
            />
          </div>
          <div className="set-fld">
            <label className="lbl">max_tokens</label>
            <input
              className="pg-field num"
              type="number"
              defaultValue={ov.max_tokens ?? ''}
              placeholder="默认"
              onBlur={(e) => onPatch(model, { max_tokens: numOrNull(e.target.value) })}
            />
          </div>
          <div className="set-fld">
            <label className="lbl">top_p</label>
            <input
              className="pg-field num"
              type="number"
              step="0.05"
              min="0"
              max="1"
              defaultValue={ov.top_p ?? ''}
              placeholder="默认"
              onBlur={(e) => onPatch(model, { top_p: numOrNull(e.target.value) })}
            />
          </div>
        </div>
        <div className="set-row">
          <label className="lbl">记忆注入</label>
          <select
            className="pg-field"
            value={ov.memory_injection ?? ''}
            onChange={(e) =>
              onPatch(model, {
                memory_injection: (e.target.value || null) as ipc.MemoryStrategy | null,
              })
            }
          >
            <option value="">默认(跟随会话)</option>
            {MEMORY_STRATEGY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
        <div className="set-row">
          <label className="lbl">默认思考强度</label>
          <select
            className="pg-field"
            value={ov.default_reasoning ?? ''}
            onChange={(e) => onPatch(model, { default_reasoning: e.target.value || null })}
            title="该模型新会话的初始思考档位;会话内手动调整后以会话选择为准"
          >
            <option value="">默认(Medium)</option>
            <option value="light">Light</option>
            <option value="medium">Medium</option>
            <option value="high">High</option>
            <option value="ultra">Ultra</option>
          </select>
        </div>
        <div className="set-row">
          <label className="lbl">思考能力</label>
          <select
            className="pg-field"
            value={capSel ?? ''}
            disabled={capSel === null}
            onChange={(e) => void applyCap(e.target.value)}
            title="内置规则表按模型名前缀推断;若推断不符(如新模型/中转站),可在此手动修正,立即生效"
          >
            {capSel === null && <option value="">加载中…</option>}
            {THINK_CAP_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
        <div className="mcard-meta">
          {card ? (
            <>
              官方 ID <code>{card.official_model_id}</code>
              {card.max_output != null && <> · 最大输出 {ipc.formatTokens(card.max_output)}</>}
              {card.tags.length > 0 && <> · {card.tags.join(' / ')}</>}
              {hasPerPrice ? (
                <>
                  {' · 价格 '}
                  {p?.per_image != null
                    ? `${p.per_image} 元/张`
                    : `${p?.per_char ?? '-'} 元/字符`}
                </>
              ) : hasTokenPrice ? (
                <>
                  {' · 价格 '}入 {inPrice ?? '-'} / 出 {p?.output ?? '-'} 元 / 百万 tokens
                  {p?.input_cache_hit != null && p.input_cache_hit !== inPrice && (
                    <>(缓存命中 {p.input_cache_hit})</>
                  )}
                </>
              ) : (
                <span className="price-missing"> · 价格未配置(可手动补充)</span>
              )}
            </>
          ) : (
            <>
              官方 ID <code>{model}</code>
              <span className="price-missing"> · 厂家未提供额度/价格,可手动补充</span>
            </>
          )}
        </div>
        {editingPrice ? (
          <CardPriceEditor
            providerId={providerId}
            model={model}
            card={card}
            onSaved={() => {
              setEditingPrice(false);
              onSaveCard();
            }}
            onCancel={() => setEditingPrice(false)}
          />
        ) : (
          <div className="price-edit-entry">
            <button className="btn-ghost btn-xs" onClick={() => setEditingPrice(true)}>
              <IconEdit size={12} />
              {hasAnyPrice ? '编辑价格与额度' : '补充价格与额度'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

/** 服务商卡片(迭代 D8):API Key 配置 + 从厂家拉取 + 内含可折叠单模型卡;支持编辑/删除。 */






