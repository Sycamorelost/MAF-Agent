import { Fragment, useEffect, useMemo, useRef, useState, type ComponentType, type ReactElement } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getVersion } from '@tauri-apps/api/app';
import logoUrl from '../assets/logo.png';
import * as ipc from '../lib/ipc';
import { ResizeHandle } from '../components/ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
import {
  IconBot,
  IconBrain,
  IconGlobe,
  IconArchive,
  IconLogo,
  IconCheck,
  IconTranslate,
  IconPlus,
  IconTrash,
  IconUpload,
  IconChevronDown,
  IconRefresh,
  IconQuote,
  IconChart,
  IconEdit,
} from '../components/icons';

const ENGINES = [
  { id: 'bocha', name: '博查 Bocha' },
  { id: 'zhipu', name: '智谱 BigModel' },
  { id: 'searxng', name: 'SearXNG' },
  { id: 'tavily', name: 'Tavily' },
];

type NavItem = { id: string; label: string; icon: ComponentType<{ size?: number }> };
type NavGroup = { grp: string; items: NavItem[] };

const NAV: NavGroup[] = [
  {
    grp: '模型',
    items: [
      { id: 'providers', label: '模型服务商', icon: IconBot },
      { id: 'memory', label: '记忆与提炼', icon: IconBrain },
    ],
  },
  {
    grp: '能力',
    items: [
      { id: 'search', label: '联网搜索', icon: IconGlobe },
      { id: 'translate', label: '翻译', icon: IconTranslate },
      { id: 'selection', label: '划词提问', icon: IconQuote },
    ],
  },
  {
    grp: '数据',
    items: [
      { id: 'usage', label: '用量统计', icon: IconChart },
      { id: 'storage', label: '存储位置', icon: IconArchive },
      { id: 'about', label: '关于', icon: IconLogo },
    ],
  },
];

/** 记忆注入策略选项(与 MemoryStrategy 对齐)。 */
const MEMORY_STRATEGY_OPTIONS: { value: ipc.MemoryStrategy; label: string }[] = [
  { value: 'full', label: '全文' },
  { value: 'summary', label: '仅摘要' },
  { value: 'hybrid', label: '摘要 + 事实 + 待办' },
  { value: 'none', label: '不注入' },
];

/** 思考能力手动修正选项(v1.9.0 工作块 I;value → ReasoningCapability,auto = 清除修正回内置推断)。 */
const THINK_CAP_OPTIONS: { value: string; label: string; cap: ipc.ReasoningCapability | null }[] = [
  { value: 'auto', label: '自动(按规则表推断)', cap: null },
  { value: 'full', label: '支持 · 可关 · 有档位', cap: { enabled: true, can_disable: true, levels: true } },
  { value: 'toggle', label: '支持 · 可关 · 仅开关', cap: { enabled: true, can_disable: true, levels: false } },
  { value: 'locked_levels', label: '支持 · 不可关 · 有档位', cap: { enabled: true, can_disable: false, levels: true } },
  { value: 'locked_toggle', label: '支持 · 不可关 · 仅开关', cap: { enabled: true, can_disable: false, levels: false } },
  { value: 'off', label: '不支持思考', cap: { enabled: false, can_disable: false, levels: false } },
];

/** ReasoningCapability → 选项 value(设置页回显用)。 */
function capToValue(cap: ipc.ReasoningCapability | null): string {
  if (!cap) return 'auto';
  if (!cap.enabled) return 'off';
  if (cap.can_disable) return cap.levels ? 'full' : 'toggle';
  return cap.levels ? 'locked_levels' : 'locked_toggle';
}

/** 数字输入 → number | null(空串/非法值视为不覆盖)。 */
function numOrNull(v: string): number | null {
  const t = v.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

/** 成本档位选项。 */
const COST_TIER_OPTIONS = Object.entries(ipc.COST_TIER_LABEL).map(([value, label]) => ({
  value,
  label,
}));

/** 价格与额度编辑器:厂家拉取缺价时的手动兜底,保存到 data/model_cards.json。 */
function CardPriceEditor({
  providerId,
  model,
  card,
  onSaved,
  onCancel,
}: {
  providerId: string;
  model: string;
  card: ipc.ModelCard | undefined;
  onSaved: () => void;
  onCancel: () => void;
}) {
  const p = card?.pricing;
  const [ctx, setCtx] = useState(card?.context_window != null ? String(card.context_window) : '');
  const [maxOut, setMaxOut] = useState(
    card?.max_output != null ? String(card.max_output) : '',
  );
  const [tier, setTier] = useState(card?.cost_tier ?? 'standard');
  const [input, setInput] = useState(p?.input != null ? String(p.input) : '');
  const [output, setOutput] = useState(p?.output != null ? String(p.output) : '');
  const [hit, setHit] = useState(
    p?.input_cache_hit != null ? String(p.input_cache_hit) : '',
  );
  const [miss, setMiss] = useState(
    p?.input_cache_miss != null ? String(p.input_cache_miss) : '',
  );
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState('');

  const save = async () => {
    setErr('');
    const inP = numOrNull(input);
    const outP = numOrNull(output);
    const hitP = numOrNull(hit);
    const missP = numOrNull(miss);
    let pricing: ipc.ModelPricing | null = null;
    if (
      inP != null ||
      outP != null ||
      hitP != null ||
      missP != null ||
      p?.per_image != null ||
      p?.per_char != null
    ) {
      pricing = {
        input: inP,
        output: outP,
        input_cache_hit: hitP,
        input_cache_miss: missP,
        per_image: p?.per_image ?? null,
        per_char: p?.per_char ?? null,
        note: p?.note ?? null,
      };
    }
    const next: ipc.ModelCard = {
      id: card?.id ?? ipc.vendorCardId(providerId, model),
      display_name: card?.display_name || model,
      provider: providerId,
      official_model_id: card?.official_model_id ?? model,
      context_window: numOrNull(ctx),
      max_output: numOrNull(maxOut),
      cost_tier: tier,
      tags: card?.tags ?? [],
      pricing,
      defaults: card?.defaults ?? null,
      memory_injection: card?.memory_injection ?? null,
    };
    setSaving(true);
    try {
      await ipc.saveModelCard(next);
      onSaved();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="price-edit">
      <div className="set-row">
        <div className="set-fld">
          <label className="lbl">上下文窗口</label>
          <input
            className="pg-field num"
            type="number"
            min="1"
            value={ctx}
            placeholder="如 1048576"
            onChange={(e) => setCtx(e.target.value)}
          />
        </div>
        <div className="set-fld">
          <label className="lbl">最大输出</label>
          <input
            className="pg-field num"
            type="number"
            min="1"
            value={maxOut}
            placeholder="如 393216"
            onChange={(e) => setMaxOut(e.target.value)}
          />
        </div>
        <div className="set-fld">
          <label className="lbl">成本档位</label>
          <select className="pg-field" value={tier} onChange={(e) => setTier(e.target.value)}>
            {COST_TIER_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
      </div>
      <div className="set-row">
        <div className="set-fld">
          <label className="lbl">输入价</label>
          <input
            className="pg-field num"
            type="number"
            min="0"
            step="0.01"
            value={input}
            placeholder="未命中缓存"
            onChange={(e) => setInput(e.target.value)}
          />
        </div>
        <div className="set-fld">
          <label className="lbl">输出价</label>
          <input
            className="pg-field num"
            type="number"
            min="0"
            step="0.01"
            value={output}
            onChange={(e) => setOutput(e.target.value)}
          />
        </div>
        <div className="set-fld">
          <label className="lbl">缓存命中价</label>
          <input
            className="pg-field num"
            type="number"
            min="0"
            step="0.01"
            value={hit}
            onChange={(e) => setHit(e.target.value)}
          />
        </div>
        <div className="set-fld">
          <label className="lbl">缓存未命中价</label>
          <input
            className="pg-field num"
            type="number"
            min="0"
            step="0.01"
            value={miss}
            onChange={(e) => setMiss(e.target.value)}
          />
        </div>
      </div>
      <div className="set-hint price-edit-hint">
        单价单位:元 / 百万 tokens,留空表示未知;缓存未命中价通常与输入价一致。保存后用于用量成本统计。
      </div>
      <div className="set-actions">
        <button className="btn-primary" disabled={saving} onClick={save}>
          <IconCheck size={13} />
          {saving ? '保存中…' : '保存价格'}
        </button>
        <button className="btn-ghost" disabled={saving} onClick={onCancel}>
          取消
        </button>
        {err && <span className="price-edit-err">{err}</span>}
      </div>
    </div>
  );
}

/** 单模型可折叠配置卡(迭代 D8):显示名/排序/默认生成参数/记忆注入策略/价格与额度。 */
function ModelConfigCard({
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
function ProviderSettingsCard({
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

/* ---------------- 关于页:MAF Agent 使用条款(v1.9.0,对标 Microsoft 使用条款组织逻辑) ---------------- */

/** 条款编号中文序号(一~十五) */
const CN_NUM = ['', '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '十一', '十二', '十三', '十四', '十五'];

type TermItem = { text: string; bullets?: string[] };
const TERMS: { no: number; name: string; items: TermItem[] }[] = [
  {
    no: 1,
    name: '条款的接受与修订',
    items: [
      {
        text: '1.1 本使用条款(以下简称"本条款")是您(以下简称"用户"或"您")与 MAF Agent 开发者梧桐吾桐及其团队(以下简称"开发者")之间,就 MAF Agent 软件(以下简称"本软件")的安装、运行与使用所订立的协议。',
      },
      {
        text: '1.2 您在安装、运行或以任何方式使用本软件前,应完整阅读本条款。您安装、运行或继续使用本软件,即视为您已阅读、理解并同意受本条款约束。若您不同意本条款的任何内容,请勿安装或使用本软件,并应将其卸载。',
      },
      {
        text: '1.3 开发者保留不时修订本条款的权利。修订时,开发者将更新本页顶部的"最近更新日期"。修订后的条款自标注日期起随新版本生效;您在修订版本发布后继续使用本软件,即视为您同意修订后的条款。',
      },
    ],
  },
  {
    no: 2,
    name: '定义',
    items: [
      { text: '2.1 "本软件"指 MAF Agent 桌面应用程序,及其附带的界面、文档与本地功能模块。' },
      {
        text: '2.2 "模型服务商"指由用户自行配置、凭 API Key 接入的第三方大模型服务提供方,包括用户自行添加的兼容接口。',
      },
      {
        text: '2.3 "用户数据"指用户使用本软件过程中在本机产生或保存的数据,包括会话记录、消息、记忆文件、知识库笔记、附件、生成图片及配置信息。',
      },
      { text: '2.4 "API Key"指用户向模型服务商申请、用于调用其模型服务的身份凭证。' },
    ],
  },
  {
    no: 3,
    name: '软件描述与授权范围',
    items: [
      {
        text: '3.1 本软件是一款专为学习、调研而创造的智能体框架软件,提供本地会话、文件即记忆、专项研究、知识库、翻译、绘画、划词提问等框架功能。本软件自身不生产、不持有大模型内容,模型能力由用户自行配置的模型服务商提供。',
      },
      {
        text: '3.2 在您遵守本条款的前提下,开发者授予您一项有限的、可撤销的、非独占的、不可转让的许可,仅供您个人为学习与调研目的安装和使用本软件。(免费与无广告承诺见第十五条。)',
      },
      {
        text: '3.3 未经开发者事先书面许可,您不得将本软件或其任何部分用于商业用途,包括但不限于商业转售、出租、有偿托管、捆绑销售,或将其部署于商业产品与商业服务中。本条款未明确授予您的所有权利,均由开发者保留。',
      },
    ],
  },
  {
    no: 4,
    name: '第三方技术依赖与开源组件',
    items: [
      {
        text: '4.1 本软件基于开源技术构建,下列组件的著作权归各自权利人所有,其使用受各自随附许可证的约束:',
        bullets: [
          '应用与界面框架:Tauri、React、React DOM、React Router、Zustand、TypeScript、Vite;',
          '本地数据:SQLite(经 rusqlite 以 bundled 方式编入,您无需另行安装数据库);',
          '网络与本地调度:reqwest(采用 rustls)、futures、notify、tokio(Tauri 运行时);',
          '加密与编码:操作系统提供的 Windows DPAPI,以及 aes-gcm、sha2、base64;',
          '文档解析与图像:docx-rs、calamine、pdf-extract、image;',
          '内容渲染:react-markdown、remark-gfm、rehype-highlight、highlight.js、mermaid、echarts。',
        ],
      },
      {
        text: '4.2 本软件在 Windows 上运行时使用系统提供的 WebView 运行时。该运行时由其权利人提供并适用其相应条款,不属于本软件的组成部分。',
      },
      {
        text: '4.3 上述组件及全部依赖的具体名称与版本,以本软件源码工程内的清单文件(Cargo.toml、package.json)及各组件自带的许可证文本为准。如任一组件条款与您的使用场景存在冲突,应以该组件条款为准并停止相关使用。',
      },
      {
        text: '4.4 开发者尊重知识产权。如您认为本软件包含的内容侵犯您的合法权益,可通过第十二条载明的联系方式提交权属证明与事实说明,开发者将在核实后依法处理。',
      },
    ],
  },
  {
    no: 5,
    name: '第三方模型服务与 API Key',
    items: [
      {
        text: '5.1 本软件不提供任何大模型服务。您须自行向模型服务商注册、取得 API Key,并自行承担该服务商按其规则收取的费用。模型服务是否可用、输出内容质量、计费方式及对您数据的处理,均受您与该服务商之间协议及其服务条款、隐私政策的约束。',
      },
      {
        text: '5.2 开发者不控制、不背书任何模型服务商及其输出内容,不对第三方服务的可用性、准确性、及时性或合法性作出任何陈述或保证。',
      },
      {
        text: '5.3 您应妥善保管 API Key,并对在您的 Key 下发生的全部调用及由此产生的费用承担全部责任。因您保管不善、Key 泄露或自行向他人披露所造成的损失,开发者不承担责任。',
      },
      {
        text: '5.4 模型请求由您的设备直接发送至您所配置的模型服务商;开发者不经手、不中转、不存储上述请求内容。',
      },
    ],
  },
  {
    no: 6,
    name: '数据存储与隐私',
    items: [
      {
        text: '6.1 本软件为本地优先应用。用户数据保存于您的本机,保存位置由您在首次启动引导或设置中自行选定,您可在设置中查看与更换数据目录。',
      },
      {
        text: '6.2 开发者不收集、不上传、不托管用户数据;本软件不包含由开发者运营的云端存储或账号体系。用户数据的保管、备份与保密由您自行负责。',
      },
      {
        text: '6.3 API Key 在 Windows 上通过操作系统提供的 DPAPI 机制加密后保存于本机,本软件任何界面均不回显 Key 的明文;旧版本以其他方式保存的密钥文件按兼容方式读取。',
      },
      {
        text: '6.4 与模型服务商之间的网络通信发生在您的设备与该服务商之间,通信内容可能被其按自身隐私政策处理,开发者对此不承担责任。',
      },
      { text: '6.5 卸载本软件或删除其数据目录可能导致用户数据不可恢复,请您事先自行备份。' },
    ],
  },
  {
    no: 7,
    name: '用户行为规范',
    items: [
      {
        text: '7.1 作为您使用本软件的条件,您承诺遵守所适用的法律法规,不得利用本软件从事任何违法或侵权行为,包括但不限于:',
        bullets: [
          '(1) 生成、复制、传播含有危害国家安全、破坏社会稳定、淫秽色情、暴力恐怖或其他法律法规禁止内容的信息;',
          '(2) 侵害他人名誉权、隐私权、知识产权或其他合法权益;',
          '(3) 对模型服务商或任何第三方系统实施未授权访问、干扰、攻击,或试图绕过其访问控制与计费规则;',
          '(4) 对本软件实施反向工程后用于侵权分发,或移除、篡改软件中的署名与权利声明;',
          '(5) 超出第三条授权范围的商业使用。',
        ],
      },
      {
        text: '7.2 大模型输出可能包含错误、过时或带有偏见的内容。您在将其用于学习、研究或其他决策前,应自行核实其准确性与适用性。',
      },
    ],
  },
  {
    no: 8,
    name: '知识产权与源码获取',
    items: [
      {
        text: '8.1 本软件的著作权及其他知识产权归开发者所有。本软件源码不随安装包公开提供;如需获取源码用于学习或改造升级,适用第十五条 15.3 的约定。',
      },
      {
        text: '8.2 获取源码不等于获得商业使用授权。您转载、分发本软件或其源码时,应保留本条款中的署名与权利声明,不得隐去或篡改软件来源。',
      },
      {
        text: '8.3 本软件界面使用的图标、文案等素材的权利归开发者所有;第三方开源组件的权利归属适用第四条。',
      },
    ],
  },
  {
    no: 9,
    name: '生成内容',
    items: [
      {
        text: '9.1 您对输入本软件的全部内容享有相应权利并承担责任,并保证其使用不侵犯任何第三方权益。',
      },
      {
        text: '9.2 模型生成内容的权属与使用限制,适用您与模型服务商之间的约定及相关法律规定。开发者不对生成内容主张所有权,亦不对其合法性、准确性或可商用性作出保证。',
      },
      { text: '9.3 您因使用、复制、传播生成内容而产生的纠纷与责任,由您自行承担。' },
    ],
  },
  {
    no: 10,
    name: '免责声明',
    items: [
      {
        text: '10.1 本软件按"现状"和"现有"基础免费提供。在适用法律允许的最大范围内,开发者不就本软件作出任何明示或默示的保证,包括但不限于对适销性、特定用途适用性、不侵权、运行不中断或不存在错误的保证。',
      },
      {
        text: '10.2 开发者不保证本软件与所有模型服务商接口持续兼容,亦不保证模型服务始终可用。因第三方服务变更、中断、限流、接口调整或计费变化导致的功能异常,不属于开发者的责任范围。',
      },
      {
        text: '10.3 因您自行更改数据目录、删除数据文件、使用非开发者提供的版本,或未按软件说明操作而导致的数据丢失或软件异常,开发者不承担责任。',
      },
    ],
  },
  {
    no: 11,
    name: '责任限制',
    items: [
      {
        text: '11.1 在适用法律允许的最大范围内,对于因使用或无法使用本软件、依赖模型输出内容、第三方服务故障,或用户数据丢失、泄露所引起的任何直接、间接、附带或后果性损失(包括但不限于数据损失、利润损失、业务中断),开发者在任何情况下均不承担赔偿责任。',
      },
      {
        text: '11.2 本软件为免费软件,您理解并同意,前述责任限制是在免费提供软件的基础上作出的合理安排。',
      },
    ],
  },
  {
    no: 12,
    name: '其他',
    items: [
      {
        text: '12.1 本条款任何条款被有权机关认定为无效或不可执行的,不影响其余条款的效力。',
      },
      { text: '12.2 因本条款或使用本软件所产生的争议,双方应首先友好协商解决。' },
      { text: '12.3 联系方式:QQ 1425408561。' },
    ],
  },
  {
    no: 13,
    name: '科研增强功能与技能来源',
    items: [
      {
        text: '13.1 本软件研究功能中的「科研增强」能力,其方法论基于开源项目 nature-skills(作者 Yuan1z0825,项目地址:https://github.com/Yuan1z0825/nature-skills ,本功能对应其「其他 agent 场景」章节:https://github.com/Yuan1z0825/nature-skills#55-%E5%85%B6%E4%BB%96-agent-%E5%9C%BA%E6%99%AF )。上述技能方法论文档经本地化重构后随本软件内置,仅供本软件本地运行使用;其著作权归原作者所有。',
      },
      {
        text: '13.2 科研增强功能的检索、生成、审计与导出全部在本机完成;本软件未移植、未启用该项目文档中面向任何第三方服务的路由,亦不为其建立任何网络连接。',
      },
    ],
  },
  {
    no: 14,
    name: '输出格式边界',
    items: [
      {
        text: '14.1 研究报告导出仅提供 Markdown 与 HTML 两种格式,全部在本机生成。本软件不提供 PPTX、DOCX、PDF 等其他格式的导出,以保障输出的稳定性与质量。',
      },
      {
        text: '14.2 本软件图示能力包括本地矢量图渲染与用户自行配置的模型服务图像生成;AI 生成图像仅作为概念草稿,不构成实验证据。',
      },
    ],
  },
  {
    no: 15,
    name: '无广告与去商业化声明',
    items: [
      {
        text: '15.1 本软件完全免费,不提供任何第三方广告服务,不包含任何原生广告、推广位或赞助内容,亦无内购项目。',
      },
      {
        text: '15.2 本软件现在不会、将来也不会参与任何商业化内容或商业化运作,将持续坚持开源免费的发展方向。',
      },
      {
        text: '15.3 本软件源码不随安装包公开提供。如需获取源码用于学习或改造升级,请联系开发者本人(QQ 1425408561)并说明用途;开发者将评估申请者是否具备软件改造升级能力以及是否存在滥用软件的倾向,经确认后方可提供。未经开发者同意,源码获取者不得公开、转发源码,不得将其用于商业用途。',
      },
    ],
  },
];

export default function SettingsPage() {
  const [searchParams] = useSearchParams();
  const initialNav = searchParams.get('tab') ?? 'providers';
  const [nav, setNav] = useState(initialNav);
  // 关于页版本号:运行期读 tauri.conf.json(单一事实源,避免多处硬编码漂移)
  const [appVersion, setAppVersion] = useState('');
  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(''));
  }, []);
  const navPanel = usePanelWidth('settings-nav', 200, 160, 320);

  // 已在设置页时,再次点击「设置」通过 query 切分区
  useEffect(() => {
    const tab = searchParams.get('tab');
    if (tab) setNav(tab);
  }, [searchParams]);

  const [templates, setTemplates] = useState<ipc.ProviderTemplate[]>([]);
  const [storedIds, setStoredIds] = useState<string[]>([]);
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [savingKey, setSavingKey] = useState<string | null>(null);

  // API 流转:多 Key 槽位 + 轮换配置
  const [rotation, setRotation] = useState<ipc.RotationConfigs>({});
  const [slotKeys, setSlotKeys] = useState<Record<string, ipc.KeySlotView[]>>({});

  // 用量统计
  const [usageDim, setUsageDim] = useState<'model' | 'provider' | 'key' | 'agent'>('model');
  const [usageDays, setUsageDays] = useState(30);
  const [usage, setUsage] = useState<ipc.UsageSummary | null>(null);
  const [usageList, setUsageList] = useState<ipc.UsageEntry[]>([]);
  const [usageHours, setUsageHours] = useState<ipc.UsageHour[]>([]);
  const [usageHealth, setUsageHealth] = useState<Record<string, ipc.UsageGroup[]>>({});
  /** 下钻:点分类条后按该分组键过滤明细表(null = 不过滤) */
  const [usageDrill, setUsageDrill] = useState<string | null>(null);

  // 模板增删(用户拥有完全权限:内置模板亦可删除,也可自行新增)
  const [savingTpl, setSavingTpl] = useState<string | null>(null);
  const [tplMsg, setTplMsg] = useState<string | null>(null);
  const [addingTpl, setAddingTpl] = useState(false);
  const [newTpl, setNewTpl] = useState({ display_name: '', base_url: '', modelsText: '' });

  // 单模型配置(迭代 D8)
  const [modelConfig, setModelConfig] = useState<ipc.ProviderModelConfig>({});
  /** 与 modelConfig 同步的即时快照:连续编辑时避免读到过期 state 而互相覆盖 */
  const modelConfigRef = useRef<ipc.ProviderModelConfig>({});
  const [cards, setCards] = useState<ipc.ModelCard[]>([]);
  const [configMsg, setConfigMsg] = useState<string | null>(null);
  const [fetching, setFetching] = useState<Record<string, boolean>>({});
  const [fetchMsg, setFetchMsg] = useState<Record<string, string>>({});

  const [settings, setSettings] = useState<ipc.AppSettings | null>(null);
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsSaved, setSettingsSaved] = useState(false);

  // 数据目录(可自定义)
  const [dataDir, setDataDir] = useState<ipc.DataDirInfo | null>(null);
  const [dirMsg, setDirMsg] = useState<string | null>(null);
  const [dirBusy, setDirBusy] = useState(false);
  const [dirRestarting, setDirRestarting] = useState(false);

  const [searchCfg, setSearchCfg] = useState<ipc.SearchConfigDto | null>(null);
  const [searchDraft, setSearchDraft] = useState({ engine: 'searxng', base_url: '', api_key: '' });
  const [savingSearch, setSavingSearch] = useState(false);
  const [searchTestStatus, setSearchTestStatus] = useState<string | null>(null);
  const [searchTesting, setSearchTesting] = useState(false);

  // 翻译术语表(迭代 D6.1)
  const [glossary, setGlossary] = useState<ipc.GlossaryEntry[] | null>(null);
  const [glossaryDirty, setGlossaryDirty] = useState(false);
  const [glossaryMsg, setGlossaryMsg] = useState<string | null>(null);
  const [savingGlossary, setSavingGlossary] = useState(false);
  const importRef = useRef<HTMLInputElement | null>(null);

  // AI 指令库(迭代 B3 / F1):自定义指令在本地编辑,保存时 upsert 全量并补删已移除项
  const [textCmds, setTextCmds] = useState<ipc.TextCommand[] | null>(null);
  const [cmdMsg, setCmdMsg] = useState<string | null>(null);
  const customCmdIdsRef = useRef<string[]>([]);

  // 加载
  useEffect(() => {
    ipc.listProviderTemplates().then(setTemplates);
    ipc.listStoredProviderIds().then((ids) => {
      setStoredIds(ids);
      // 预加载各 Provider 的 Key 槽位(掩码/备注/启用/冷却),供多 Key 面板展示
      ids.forEach((id) => void loadSlots(id));
    });
    ipc.getRotationConfigs().then(setRotation).catch(() => {});
    ipc.getSettings().then((s) => setSettings(s));
    ipc.getDataDir().then(setDataDir).catch(() => {});
    ipc.getProviderModelConfig().then((c) => {
      modelConfigRef.current = c;
      setModelConfig(c);
    }).catch(() => {});
    ipc.listModelCards().then(setCards).catch(() => {});
    ipc.getSearchConfig().then((c) => {
      setSearchCfg(c);
      setSearchDraft((d) => ({ ...d, engine: c.engine, base_url: c.base_url }));
    });
    ipc.glossaryList().then(setGlossary);
    ipc.listTextCommands().then((list) => {
      customCmdIdsRef.current = list.filter((c) => !c.builtin).map((c) => c.id);
      setTextCmds(list);
    });
  }, []);

  const keyedTemplates = useMemo(
    () => templates.filter((t) => storedIds.includes(t.id)),
    [templates, storedIds],
  );

  const engineLabel = ENGINES.find((e) => e.id === searchCfg?.engine)?.name ?? '未配置';
  const engineReady = searchCfg
    ? searchCfg.engine === 'searxng'
      ? !!searchCfg.base_url
      : searchCfg.has_api_key
    : false;

  /** 拉取某 Provider 的 Key 槽位列表(掩码 / 备注 / 启用 / 冷却)。 */
  const loadSlots = async (providerId: string) => {
    try {
      const list = await ipc.listProviderKeys(providerId);
      setSlotKeys((m) => ({ ...m, [providerId]: list }));
    } catch {
      /* 忽略:渲染时按空列表降级 */
    }
  };

  /** 新增一把 Key(自动分配到新槽位;备注写入槽位元数据)。 */
  const onSaveKey = async (providerId: string, label: string) => {
    const key = keyInputs[providerId] ?? '';
    if (!key) return;
    setSavingKey(providerId);
    try {
      await ipc.saveProviderKey(providerId, key, label);
      setStoredIds((ids) => (ids.includes(providerId) ? ids : [...ids, providerId]));
      setKeyInputs((m) => ({ ...m, [providerId]: '' }));
      await loadSlots(providerId);
      setRotation(await ipc.getRotationConfigs());
    } finally {
      setSavingKey(null);
    }
  };

  /** 更新槽位元数据(备注 / 启用 / 权重 / 优先级;未传字段保持原值)。 */
  const onPatchSlot = async (
    providerId: string,
    slot: number,
    patch: { label?: string; enabled?: boolean; weight?: number; priority?: number },
  ) => {
    const cur = (slotKeys[providerId] ?? []).find((s) => s.slot === slot);
    await ipc.updateKeySlot(
      providerId,
      slot,
      patch.label ?? cur?.label ?? '',
      patch.enabled ?? cur?.enabled ?? true,
      patch.weight ?? cur?.weight ?? 1,
      patch.priority ?? cur?.priority ?? 0,
    );
    await loadSlots(providerId);
  };

  /** 删除单个槽位的 Key。 */
  const onDeleteSlot = async (providerId: string, slot: number) => {
    if (!confirm(`删除槽位 #${slot} 的 API Key?`)) return;
    await ipc.deleteProviderKey(providerId, slot);
    await loadSlots(providerId);
    setStoredIds(await ipc.listStoredProviderIds());
  };

  /** 更新轮换配置(启用 / 冷却 / stream_options 兼容开关)。 */
  const onPatchRotation = async (
    providerId: string,
    patch: { enabled?: boolean; cooldown_secs?: number; supports_stream_options?: boolean },
  ) => {
    const cur = rotation[providerId];
    const next = {
      enabled: patch.enabled ?? cur?.enabled ?? false,
      cooldown_secs: patch.cooldown_secs ?? cur?.cooldown_secs ?? 60,
      supports_stream_options: patch.supports_stream_options ?? cur?.supports_stream_options ?? true,
    };
    await ipc.setProviderRotation(
      providerId,
      next.enabled,
      next.cooldown_secs,
      next.supports_stream_options,
    );
    setRotation(await ipc.getRotationConfigs());
  };

  const onClearKey = async (providerId: string) => {
    if (!confirm(`清除 ${providerId} 的全部 API Key?`)) return;
    await ipc.deleteApiKey(providerId);
    setStoredIds((ids) => ids.filter((x) => x !== providerId));
    await loadSlots(providerId);
  };

  // ---- 用量统计 ----

  const loadUsage = async (dim: 'model' | 'provider' | 'key' | 'agent', days: number) => {
    const [sum, list, hours] = await Promise.all([
      ipc.usageSummary(dim, days),
      ipc.usageEntries(100),
      ipc.usageHourly(days),
    ]);
    setUsage(sum);
    setUsageList(list);
    setUsageHours(hours);
    // Key 健康度:按已有 Key 的 Provider 逐个拉取(供「Key 健康」表展示)
    const ids = await ipc.listStoredProviderIds();
    const pairs = await Promise.all(
      ids.map(async (id) => [id, await ipc.usageKeyHealth(id, days).catch(() => [])] as const),
    );
    setUsageHealth(Object.fromEntries(pairs));
  };

  useEffect(() => {
    if (nav !== 'usage') return;
    setUsageDrill(null);
    void loadUsage(usageDim, usageDays);
  }, [nav, usageDim, usageDays]);

  // ---- 单模型配置(迭代 D8)----

  /** 重新加载模型卡(拉取厂家模型/手动保存价格后调用)。 */
  const reloadCards = async () => {
    try {
      setCards(await ipc.listModelCards());
    } catch {
      // 忽略:卡片为展示增强,不影响主流程
    }
  };

  const onFetchModels = async (providerId: string) => {
    setFetching((s) => ({ ...s, [providerId]: true }));
    setFetchMsg((s) => ({ ...s, [providerId]: '' }));
    try {
      const models = await ipc.fetchProviderModels(providerId);
      setFetchMsg((s) => ({
        ...s,
        [providerId]: `已拉取 ${models.length} 个模型(含额度/价格等元数据,已设为该服务商模型列表)`,
      }));
      setTemplates(await ipc.listProviderTemplates());
      // 厂家元数据 + 刊例目录派生的卡片需要同步刷新
      await reloadCards();
    } catch (e) {
      setFetchMsg((s) => ({
        ...s,
        [providerId]: `拉取失败:${e instanceof Error ? e.message : String(e)}`,
      }));
    } finally {
      setFetching((s) => ({ ...s, [providerId]: false }));
    }
  };

  /** 更新某模型覆盖项:本地即时生效 + 整体落盘 + 刷新模板列表(启用/排序影响列表)。 */
  const onPatchOverride = async (
    providerId: string,
    model: string,
    patch: Partial<ipc.ModelOverride>,
  ) => {
    const cur = modelConfigRef.current;
    const prev = cur[providerId]?.[model] ?? ipc.EMPTY_MODEL_OVERRIDE;
    const next: ipc.ProviderModelConfig = {
      ...cur,
      [providerId]: { ...(cur[providerId] ?? {}), [model]: { ...prev, ...patch } },
    };
    modelConfigRef.current = next;
    setModelConfig(next);
    setConfigMsg(null);
    try {
      await ipc.saveProviderModelConfig(next);
      setTemplates(await ipc.listProviderTemplates());
      setConfigMsg('设置已保存');
    } catch (e) {
      setConfigMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // ---- 模板增删改(用户拥有完全权限:内置模板亦可删除)----

  /** 就地编辑某服务商(显示名 / Base URL / 模型列表),整体落盘后刷新。 */
  const onEditTemplate = async (providerId: string, patch: Partial<ipc.ProviderTemplate>) => {
    const tpl = templates.find((t) => t.id === providerId);
    if (!tpl) return;
    setSavingTpl(providerId);
    setTplMsg(null);
    try {
      await ipc.saveProviderTemplate({ ...tpl, all_models: [], ...patch });
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg('已保存');
    } catch (e) {
      setTplMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 新增服务商(OpenAI 兼容协议,id 由后端生成)。 */
  const onAddTemplate = async () => {
    setSavingTpl('__new__');
    setTplMsg(null);
    try {
      const saved = await ipc.saveProviderTemplate({
        id: '',
        display_name: newTpl.display_name.trim() || '新服务商',
        base_url: newTpl.base_url.trim(),
        models: newTpl.modelsText.split('\n').map((s) => s.trim()).filter(Boolean),
        all_models: [],
        image_models: [],
        docs_url: null,
        openai_compat: true,
      });
      setTemplates(await ipc.listProviderTemplates());
      setNewTpl({ display_name: '', base_url: '', modelsText: '' });
      setAddingTpl(false);
      setTplMsg(`已新增服务商「${saved.display_name}」`);
    } catch (e) {
      setTplMsg(`新增失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 删除服务商(内置模板亦可删除),同时清理其 Key、单模型配置与模型缓存。 */
  const onDeleteTemplate = async (providerId: string, name: string) => {
    if (!confirm(`删除服务商「${name}」?将同时清除其 API Key、单模型配置与模型缓存。`)) return;
    setSavingTpl(providerId);
    setTplMsg(null);
    try {
      await ipc.deleteProviderTemplate(providerId);
      setStoredIds((ids) => ids.filter((x) => x !== providerId));
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg(`已删除「${name}」`);
    } catch (e) {
      setTplMsg(`删除失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingTpl(null);
    }
  };

  /** 补回被删除的内置模板(仅补缺失项,不影响用户已编辑/新增的模板)。 */
  const onRestoreBuiltins = async () => {
    setTplMsg(null);
    try {
      await ipc.restoreBuiltinTemplates();
      setTemplates(await ipc.listProviderTemplates());
      setTplMsg('已补回缺失的内置模板');
    } catch (e) {
      setTplMsg(`恢复失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const onSaveSettings = async () => {
    if (!settings) return;
    setSavingSettings(true);
    setSettingsSaved(false);
    try {
      await ipc.saveSettings(settings);
      setSettingsSaved(true);
    } finally {
      setSavingSettings(false);
    }
  };

  /** 更换数据目录:选目录 → 确认(提示旧目录将被删除) → 写配置 → 重启迁移。 */
  const onChangeDataDir = async () => {
    setDirMsg(null);
    try {
      const p = await ipc.pickDataDir();
      if (!p) return;
      if (dataDir && p.toLowerCase() === dataDir.path.toLowerCase()) {
        setDirMsg('所选目录与当前目录相同,无需更改');
        return;
      }
      const oldTip = dataDir?.path ? `\n\n迁移完成后原目录\n${dataDir.path}\n将被删除。` : '';
      if (!confirm(`确定把数据目录更改为:\n${p}${oldTip}\n\n应用将自动重启以完成迁移。`)) return;
      setDirBusy(true);
      const r = await ipc.setDataDir(p, true);
      if (r.restart_required) {
        setDirRestarting(true);
        await ipc.restartApp();
      } else {
        setDataDir(await ipc.getDataDir());
        setDirMsg('数据目录已更新(无需重启)');
      }
    } catch (e) {
      setDirMsg(`更改失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDirBusy(false);
    }
  };

  const onOpenDataDir = async () => {
    setDirMsg(null);
    try {
      await ipc.openDataDir();
    } catch (e) {
      setDirMsg(`打开失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const onSaveSearch = async () => {
    setSavingSearch(true);
    setSearchTestStatus(null);
    try {
      await ipc.saveSearchConfig({
        engine: searchDraft.engine,
        base_url: searchDraft.base_url,
        api_key: searchDraft.api_key || null,
      });
      const c = await ipc.getSearchConfig();
      setSearchCfg(c);
      setSearchDraft((d) => ({ ...d, base_url: c.base_url, api_key: '' }));
    } catch (e) {
      setSearchTestStatus(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingSearch(false);
    }
  };

  const onTestSearch = async () => {
    setSearchTesting(true);
    setSearchTestStatus('测试中…');
    try {
      const res = await ipc.searchTest('MAF Agent');
      if (res.length === 0) {
        setSearchTestStatus('测试成功,但未返回结果(可换关键词)');
      } else {
        const first = res[0];
        setSearchTestStatus(`测试成功,返回 ${res.length} 条;示例:${first.title || first.url}`);
      }
    } catch (e) {
      setSearchTestStatus(`测试失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSearchTesting(false);
    }
  };

  // ---- 术语表(迭代 D6.1)----

  const mutateGlossary = (next: ipc.GlossaryEntry[]) => {
    setGlossary(next);
    setGlossaryDirty(true);
    setGlossaryMsg(null);
  };

  const onSaveGlossary = async (next?: ipc.GlossaryEntry[]) => {
    const list = next ?? glossary ?? [];
    setSavingGlossary(true);
    setGlossaryMsg(null);
    try {
      await ipc.glossarySave(list);
      setGlossary(list.filter((e) => e.src.trim() && e.dst.trim()));
      setGlossaryDirty(false);
      setGlossaryMsg(`已保存 ${list.filter((e) => e.src.trim() && e.dst.trim()).length} 条术语`);
    } catch (e) {
      setGlossaryMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingGlossary(false);
    }
  };

  /** 导入 JSON 术语表(数组结构 [{src,dst,note?}];按 src 合并去重后立即保存)。 */
  const onImportGlossary = async (file: File) => {
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as unknown;
      if (!Array.isArray(parsed)) throw new Error('JSON 顶层必须是数组');
      const incoming: ipc.GlossaryEntry[] = [];
      for (const item of parsed) {
        const o = item as Record<string, unknown>;
        if (typeof o?.src === 'string' && typeof o?.dst === 'string' && o.src && o.dst) {
          incoming.push({
            src: o.src,
            dst: o.dst,
            note: typeof o.note === 'string' ? o.note : null,
          });
        }
      }
      if (incoming.length === 0) throw new Error('未解析到有效词条(需 src/dst 字段)');
      // 按 src 合并:新词条覆盖同名旧词条
      const map = new Map<string, ipc.GlossaryEntry>();
      for (const e of glossary ?? []) map.set(e.src, e);
      for (const e of incoming) map.set(e.src, e);
      const merged = [...map.values()];
      await onSaveGlossary(merged);
      setGlossaryMsg(`已导入 ${incoming.length} 条(合并后共 ${merged.length} 条)`);
    } catch (e) {
      setGlossaryMsg(`导入失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const providersView = (
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
              onKeyInput={(v) => setKeyInputs((m) => ({ ...m, [t.id]: v }))}
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
                  onChange={(e) => setNewTpl({ ...newTpl, display_name: e.target.value })}
                  placeholder="如 我的中转站"
                />
              </div>
              <div className="set-row">
                <label className="lbl">Base URL</label>
                <input
                  className="pg-field"
                  value={newTpl.base_url}
                  onChange={(e) => setNewTpl({ ...newTpl, base_url: e.target.value })}
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
                  onChange={(e) => setNewTpl({ ...newTpl, modelsText: e.target.value })}
                  placeholder={'model-a\nmodel-b'}
                  style={{ resize: 'vertical' }}
                />
              </div>
              <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
                <button className="btn-ghost" onClick={() => setAddingTpl(false)}>
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
              <button className="btn-primary" onClick={() => setAddingTpl(true)}>
                <IconPlus size={14} />
                新增服务商
              </button>
            </div>
          )}
        </div>
      </div>
    </>
  );

  const memoryView = (
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
                setSettings((s) =>
                  s ? { ...s, summarize_provider_id: e.target.value || null } : s,
                )
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
                    setSettings((s) => (s ? { ...s, summarize_model: e.target.value || null } : s))
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
    </>
  );

  const searchView = (
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
              onChange={(e) => setSearchDraft((d) => ({ ...d, engine: e.target.value }))}
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
                onChange={(e) => setSearchDraft((d) => ({ ...d, base_url: e.target.value }))}
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
              onChange={(e) => setSearchDraft((d) => ({ ...d, api_key: e.target.value }))}
              placeholder={searchCfg?.has_api_key ? '留空则保留已配置的 Key' : 'SearXNG 可留空'}
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

  // 翻译视图(迭代 D6):术语表管理
  const translateView = (
    <>
      <h2>翻译</h2>
      <p className="set-desc">
        术语表在翻译时自动注入,保证专有名词译法一致;落盘 data/glossary.json。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          术语表
          <span className="st">
            <IconCheck size={13} />
            {glossary?.length ?? 0} 条
          </span>
        </div>
        <div className="set-card-b">
          {glossary === null && <div className="set-hint">加载中…</div>}
          {glossary !== null && glossary.length === 0 && (
            <div className="set-hint" style={{ marginBottom: 'var(--space-3)' }}>
              暂无术语。添加后,翻译原文命中「原文词」时将优先采用指定译法。
            </div>
          )}
          {glossary?.map((entry, idx) => (
            <div className="set-row" key={idx} style={{ marginBottom: 'var(--space-2)' }}>
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="原文词"
                value={entry.src}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, src: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="指定译法"
                value={entry.dst}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, dst: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="备注(可选)"
                value={entry.note ?? ''}
                onChange={(e) =>
                  mutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, note: e.target.value } : x)),
                  )
                }
              />
              <button
                className="btn-ghost"
                title="删除词条"
                onClick={() => mutateGlossary(glossary.filter((_, i) => i !== idx))}
              >
                <IconTrash size={14} />
              </button>
            </div>
          ))}
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button
              className="btn-ghost"
              onClick={() => mutateGlossary([...(glossary ?? []), { src: '', dst: '', note: '' }])}
            >
              <IconPlus size={14} />
              添加词条
            </button>
            <button className="btn-ghost" onClick={() => importRef.current?.click()}>
              <IconUpload size={14} />
              导入 JSON
            </button>
            <span className="sp" />
            <button
              className="btn-primary"
              onClick={() => void onSaveGlossary()}
              disabled={savingGlossary || !glossaryDirty}
            >
              {savingGlossary ? '保存中…' : glossaryDirty ? '保存术语表' : '已保存'}
            </button>
          </div>
          {glossaryMsg && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              {glossaryMsg}
            </div>
          )}
          <input
            ref={importRef}
            type="file"
            accept=".json,application/json"
            style={{ display: 'none' }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void onImportGlossary(f);
              e.target.value = '';
            }}
          />
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">DeepL 专用引擎</div>
        <div className="set-card-b">
          <div className="set-hint">
            设置页「模型服务商」中配置 DeepL API Key 后,翻译页即可选用。DeepL 为专用翻译引擎,
            不参与对话与模型列表拉取;术语表对 DeepL 同样生效(译文通常已较稳定,术语注入以
            LLM 类引擎为主)。
          </div>
        </div>
      </div>
    </>
  );

  const storageView = (
    <>
      {dirRestarting && (
        <div className="dialog-mask">
          <div className="dialog">
            <div className="dialog-head">正在重启应用</div>
            <div className="dialog-body">
              <p className="set-hint">数据正在迁移到你选择的新目录,应用即将自动重新启动…</p>
            </div>
          </div>
        </div>
      )}
      <h2>存储位置</h2>
      <p className="set-desc">
        无账号体系,数据只存本机,统一落在下方所示的本地数据目录下(可自定义位置)。
      </p>

      <div className="set-card">
        <div className="set-card-h">当前数据目录</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">目录</label>
            <span
              style={{
                flex: 1,
                fontFamily: 'var(--font-mono)',
                color: 'var(--text-1)',
                wordBreak: 'break-all',
              }}
            >
              {dataDir?.path ?? '读取中…'}
            </span>
          </div>
          <div className="set-row">
            <label className="lbl">引导配置</label>
            <span
              className="set-hint"
              style={{ fontFamily: 'var(--font-mono)', wordBreak: 'break-all' }}
            >
              {dataDir?.config_path ?? '—'}
            </span>
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-primary" onClick={onChangeDataDir} disabled={dirBusy}>
              更改目录
            </button>
            <button className="btn-ghost" onClick={onOpenDataDir}>
              在资源管理器中打开
            </button>
            <span className="sp" />
            {dataDir?.legacy_adopted && (
              <span className="set-hint">检测到历史数据目录,建议更改到自定义位置</span>
            )}
          </div>
          {dataDir?.migration_error && (
            <div className="set-hint" style={{ color: 'var(--err)' }}>
              上次迁移失败:{dataDir.migration_error}
            </div>
          )}
          {dirMsg && (
            <div
              className="set-hint"
              style={{ color: dirMsg.includes('失败') ? 'var(--err)' : 'var(--text-2)' }}
            >
              {dirMsg}
            </div>
          )}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">内部结构</div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <table className="set-table">
            <thead>
              <tr>
                <th>内容</th>
                <th>路径</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>会话与消息</td>
                <td className="mn">data/metaagent.db</td>
              </tr>
              <tr>
                <td>会话记忆</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.md</td>
              </tr>
              <tr>
                <td>记忆快照</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.versions/</td>
              </tr>
              <tr>
                <td>附件</td>
                <td className="mn">data/attachments/&#123;session&#125;/</td>
              </tr>
              <tr>
                <td>生成图片</td>
                <td className="mn">data/images/</td>
              </tr>
              <tr>
                <td>翻译历史</td>
                <td className="mn">data/translations/</td>
              </tr>
              <tr>
                <td>术语表</td>
                <td className="mn">data/glossary.json</td>
              </tr>
              <tr>
                <td>模型卡覆盖</td>
                <td className="mn">data/model_cards.json</td>
              </tr>
              <tr>
                <td>单模型配置</td>
                <td className="mn">data/provider_model_config.json</td>
              </tr>
              <tr>
                <td>服务商模板</td>
                <td className="mn">data/provider_templates.json</td>
              </tr>
              <tr>
                <td>厂家模型缓存</td>
                <td className="mn">data/provider_models.json</td>
              </tr>
              <tr>
                <td>深度研究</td>
                <td className="mn">data/research/&#123;id&#125;/report.md</td>
              </tr>
              <tr>
                <td>密钥(加密)</td>
                <td className="mn">data/secrets/&#123;provider&#125;.bin</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  );

  const aboutView = (
    <>
      <h2>关于</h2>

      {/* 产品标识区:logo 原比例 + 名称 + 版本(读 tauri.conf.json)+ 定位语 */}
      <div className="set-card set-product">
        <img className="set-product-logo" src={logoUrl} alt="MAF Agent" />
        <div className="set-product-info">
          <div className="set-product-name">MAF Agent</div>
          <div className="set-product-ver">
            版本 {appVersion || '1.9.0'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            一款专为学习、调研而创造的智能体框架软件。MAF 旗下完全免费且无广告的 AI
            框架软件,开发者:梧桐吾桐及其团队。
          </div>
        </div>
      </div>

      {/* 条款文档区:十五条,全部展开 */}
      <div className="set-card">
        <div className="set-card-h">
          MAF Agent 使用条款
          <span className="st">最近更新日期:2026 年 10 月 17 日</span>
        </div>
        <div className="set-card-b set-terms">
          {TERMS.map((t) => (
            <section key={t.no} className="set-term">
              <h3 className="set-term-h">第{CN_NUM[t.no]}条 · {t.name}</h3>
              {t.items.map((it, i) => (
                <div key={i} className="set-term-item">
                  <p>{it.text}</p>
                  {it.bullets && (
                    <ul>
                      {it.bullets.map((b, j) => (
                        <li key={j}>{b}</li>
                      ))}
                    </ul>
                  )}
                </div>
              ))}
            </section>
          ))}
          <p className="set-term-foot">
            本条款为软件使用协议文本。开发者并非专业法律服务机构,如您对条款有疑问,建议在使用前自行咨询专业人士。
          </p>
        </div>
      </div>
    </>
  );

  // ---- 划词提问(迭代 D9)+ AI 指令库(迭代 B3 / F1)----

  const patchSettings = (patch: Partial<ipc.AppSettings>) =>
    setSettings((s) => (s ? { ...s, ...patch } : s));

  const patchTextCmd = (id: string, patch: Partial<ipc.TextCommand>) =>
    setTextCmds((list) =>
      list ? list.map((c) => (c.id === id ? { ...c, ...patch } : c)) : list,
    );

  const addTextCmd = () =>
    setTextCmds((list) =>
      list
        ? [
            ...list,
            {
              id: `custom_${Date.now().toString(36)}`,
              name: '新指令',
              icon: 'sparkles',
              prompt_template: '',
              builtin: false,
              sort: 1000 + list.length,
              scope: 'both' as const,
            },
          ]
        : list,
    );

  const removeTextCmd = (id: string) =>
    setTextCmds((list) => (list ? list.filter((c) => c.id !== id) : list));

  /** 保存:划词开关(settings)+ 指令库(upsert 自定义,删除已移除项)。 */
  const onSaveSelection = async () => {
    await onSaveSettings();
    if (!textCmds) return;
    setCmdMsg(null);
    try {
      const customs = textCmds.filter((c) => !c.builtin);
      for (const c of customs) await ipc.saveTextCommand(c);
      const kept = new Set(customs.map((c) => c.id));
      for (const oldId of customCmdIdsRef.current) {
        if (!kept.has(oldId)) await ipc.deleteTextCommand(oldId);
      }
      const list = await ipc.listTextCommands();
      customCmdIdsRef.current = list.filter((c) => !c.builtin).map((c) => c.id);
      setTextCmds(list);
      setCmdMsg('指令库已保存');
    } catch (e) {
      setCmdMsg(`指令保存失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const SCOPE_OPTIONS = [
    { value: 'both', label: '对话' },
    { value: 'chat', label: '仅对话' },
  ] as const;

  const selectionView = (
    <>
      <h2>划词提问</h2>
      <p className="set-desc">
        在任意应用中选中文本后连按两次 Ctrl+C,主窗口会被唤起并新建会话,选中文字自动填入输入框,按回车即可提问。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          触发开关
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">划词提问</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.selection_ask_enabled ?? false}
              className={`set-switch${settings?.selection_ask_enabled ? ' on' : ''}`}
              onClick={() =>
                patchSettings({ selection_ask_enabled: !settings?.selection_ask_enabled })
              }
            >
              <span className="knob" />
            </button>
            <span className="set-hint">关闭后连按两次 Ctrl+C 不再唤起主窗口</span>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">主窗口 / 托盘</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">最小化到托盘</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.minimize_to_tray ?? false}
              className={`set-switch${settings?.minimize_to_tray ? ' on' : ''}`}
              onClick={() => patchSettings({ minimize_to_tray: !settings?.minimize_to_tray })}
            >
              <span className="knob" />
            </button>
            <span className="set-hint">
              默认关:点 × 退出应用,最小化保留任务栏图标;开启后点 × 与最小化都隐藏到托盘
            </span>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          AI 指令库
          <span className="sp" />
          {cmdMsg && <span className="st">{cmdMsg}</span>}
        </div>
        <div className="set-card-b">
          <p className="set-hint" style={{ marginTop: 0 }}>
            指令会出现在对话页「指令」面板;模板支持 {'{selection}'}(划词文本)与{' '}
            {'{clipboard}'}(剪贴板)变量,留空 = 只把选中内容作为引用带入对话。
          </p>
          {(textCmds ?? []).map((c) => (
            <div className="sel-btn-row" key={c.id}>
              <input
                className="pg-field lbl-in"
                value={c.name}
                placeholder="指令名"
                spellCheck={false}
                disabled={c.builtin}
                onChange={(e) => patchTextCmd(c.id, { name: e.target.value })}
              />
              <select
                className="pg-field scope-in"
                value={c.scope}
                disabled={c.builtin}
                onChange={(e) =>
                  patchTextCmd(c.id, { scope: e.target.value as ipc.TextCommand['scope'] })
                }
              >
                {SCOPE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
              <input
                className="pg-field"
                value={c.prompt_template}
                placeholder="指令模板(留空 = 只带入引用,由你自行提问)"
                spellCheck={false}
                disabled={c.builtin}
                onChange={(e) => patchTextCmd(c.id, { prompt_template: e.target.value })}
              />
              {c.builtin ? (
                <span className="cmd-builtin-tag">内置</span>
              ) : (
                <button className="prov-del" title="删除该指令" onClick={() => removeTextCmd(c.id)}>
                  <IconTrash size={14} />
                </button>
              )}
            </div>
          ))}
          {(textCmds ?? []).length === 0 && (
            <div className="set-hint">暂无指令,点「新增指令」添加</div>
          )}
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button className="btn-ghost" onClick={addTextCmd}>
              <IconPlus size={14} />
              新增指令
            </button>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSelection} disabled={savingSettings}>
              保存划词设置
            </button>
          </div>
        </div>
      </div>
    </>
  );

  // ---- 用量统计 ----

  const DIM_OPTIONS = [
    { value: 'model' as const, label: '按模型' },
    { value: 'provider' as const, label: '按服务商' },
    { value: 'key' as const, label: '按 Key' },
    { value: 'agent' as const, label: '按助手' },
  ];

  const fmtNum = (n: number) => n.toLocaleString('en-US');

  /** 微元(1e-6 元) → 人民币展示;量级越小保留位数越多。 */
  const fmtCost = (micro: number | null | undefined) => {
    if (micro == null) return '—';
    const yuan = micro / 1e6;
    if (yuan === 0) return '¥0';
    if (yuan < 0.01) return `¥${yuan.toFixed(6)}`;
    if (yuan < 1) return `¥${yuan.toFixed(4)}`;
    return `¥${yuan.toFixed(2)}`;
  };

  /** Unix 秒 → MM-DD HH:mm。 */
  const fmtClock = (secs: number) => {
    const d = new Date(secs * 1000);
    const p = (x: number) => String(x).padStart(2, '0');
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };

  const STATUS_LABEL: Record<string, string> = {
    success: '成功',
    failed: '失败',
    rate_limited: '限流',
  };

  const KIND_LABEL: Record<string, string> = {
    stream: '流中断',
    auth: '鉴权失败',
    server: '服务端错误',
    network: '网络错误',
    bad_request: '请求错误',
    rate_limited: '限流',
    not_supported: '不支持',
    parse: '解析失败',
    other: '其他',
  };

  const usageTotals = usage?.totals;
  const usageGroups = usage?.groups ?? [];
  const maxReq = usageGroups.reduce((m, g) => (g.requests > m ? g.requests : m), 0) || 1;
  const failRate =
    usageTotals && usageTotals.requests > 0
      ? `${((usageTotals.failed / usageTotals.requests) * 100).toFixed(1)}%`
      : '0%';
  const avgFirst =
    usageTotals?.avg_first_token_ms != null ? `${usageTotals.avg_first_token_ms} ms` : '—';

  /** 明细行归属的分类键(与后端 summary_on 的 key 表达式一致),用于下钻过滤。 */
  const drillKeyOf = (e: ipc.UsageEntry) => {
    switch (usageDim) {
      case 'provider':
        return e.provider_id;
      case 'key':
        return String(e.slot_id);
      case 'agent':
        return e.agent_id || 'none';
      default:
        return e.model_id;
    }
  };
  const drillLabel = usageDrill
    ? usageGroups.find((g) => g.key === usageDrill)?.name ?? usageDrill
    : null;
  const usageListShown = usageDrill ? usageList.filter((e) => drillKeyOf(e) === usageDrill) : usageList;

  /** 热力图:星期(0=周日)× 小时,值为请求数。 */
  const heatCells = (() => {
    const grid = new Map<string, ipc.UsageHour>();
    for (const h of usageHours) grid.set(`${h.dow}-${h.hour}`, h);
    return grid;
  })();
  const heatMax = usageHours.reduce((m, h) => (h.requests > m ? h.requests : m), 0) || 1;
  const DOW_LABEL = ['日', '一', '二', '三', '四', '五', '六'];

  /** Key 健康表:各 Provider × 槽位的成功率 / 首字延迟。 */
  const healthRows = Object.entries(usageHealth).flatMap(([pid, groups]) =>
    groups.map((g) => ({ ...g, provider_id: pid })),
  );

  const usageView = (
    <>
      <h2>用量统计</h2>
      <p className="set-desc">
        统计本机请求的 Token 用量、失败率与首字延迟,可按模型 / 服务商 / API Key 分类查看。数据仅存本地。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          总览
          <span className="sp" />
          <div className="seg">
            {[7, 30, 90].map((d) => (
              <button key={d} className={usageDays === d ? 'on' : ''} onClick={() => setUsageDays(d)}>
                近 {d} 天
              </button>
            ))}
          </div>
        </div>
        <div className="set-card-b">
          <div className="usage-metrics">
            <div className="usage-metric">
              <div className="v">{fmtNum(usageTotals?.requests ?? 0)}</div>
              <div className="k">总请求数</div>
            </div>
            <div className="usage-metric">
              <div className="v">{ipc.formatTokens(usageTotals?.tokens ?? 0)}</div>
              <div className="k">总 Token 用量</div>
            </div>
            <div className="usage-metric">
              <div className="v">{failRate}</div>
              <div className="k">失败率(限流 {fmtNum(usageTotals?.rate_limited ?? 0)} 次)</div>
            </div>
            <div className="usage-metric">
              <div className="v">{avgFirst}</div>
              <div className="k">平均首字延迟</div>
            </div>
            <div className="usage-metric">
              <div className="v">{fmtCost(usageTotals?.cost_micro)}</div>
              <div className="k">总成本(按定价表估算)</div>
            </div>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          时段热力图
          <span className="sp" />
          <span className="set-hint">星期 × 小时,颜色越深请求越多</span>
        </div>
        <div className="set-card-b">
          {usageHours.length === 0 ? (
            <div className="set-hint">该时间范围内暂无用量数据</div>
          ) : (
            <div className="usage-heat">
              <div className="usage-heat-corner" />
              {Array.from({ length: 24 }, (_, h) => (
                <div className="usage-heat-hd" key={`h${h}`}>
                  {h % 3 === 0 ? h : ''}
                </div>
              ))}
              {DOW_LABEL.map((dl, dow) => (
                <Fragment key={`d${dow}`}>
                  <div className="usage-heat-dow">周{dl}</div>
                  {Array.from({ length: 24 }, (_, h) => {
                    const cell = heatCells.get(`${dow}-${h}`);
                    const v = cell?.requests ?? 0;
                    const alpha = v === 0 ? 0 : 0.15 + 0.85 * (v / heatMax);
                    return (
                      <div
                        key={`c${dow}-${h}`}
                        className="usage-heat-cell"
                        title={`周${dl} ${h}:00 · ${fmtNum(v)} 次 · ${ipc.formatTokens(cell?.tokens ?? 0)} tokens`}
                        style={v === 0 ? undefined : { background: `rgba(59,130,246,${alpha.toFixed(2)})` }}
                      />
                    );
                  })}
                </Fragment>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">
          分类统计
          <span className="sp" />
          <div className="seg">
            {DIM_OPTIONS.map((o) => (
              <button
                key={o.value}
                className={usageDim === o.value ? 'on' : ''}
                onClick={() => setUsageDim(o.value)}
              >
                {o.label}
              </button>
            ))}
          </div>
        </div>
        <div className="set-card-b">
          {usageGroups.length === 0 ? (
            <div className="set-hint">该时间范围内暂无用量数据</div>
          ) : (
            usageGroups.map((g) => (
              <div
                className={`usage-bar-row${usageDrill === g.key ? ' on' : ''}`}
                key={g.key}
                onClick={() => setUsageDrill((d) => (d === g.key ? null : g.key))}
                role="button"
                title="点击下钻:只看该分类的请求明细"
              >
                <span className="nm" title={g.name}>
                  {g.name}
                  {g.cooling && <span className="cool-num">冷却中</span>}
                </span>
                <span className="bar">
                  <i style={{ width: `${Math.max(3, Math.round((g.requests / maxReq) * 100))}%` }} />
                </span>
                <span className="num">
                  {fmtNum(g.requests)} 次 · {ipc.formatTokens(g.tokens)}
                  {g.cost_micro > 0 ? ` · ${fmtCost(g.cost_micro)}` : ''}
                  {g.failed > 0 ? ` · 失败 ${g.failed}` : ''}
                </span>
              </div>
            ))
          )}
        </div>
      </div>

      {healthRows.length > 0 && (
        <div className="set-card">
          <div className="set-card-h">Key 健康</div>
          <div className="set-card-b" style={{ padding: 0 }}>
            <div className="usage-scroll">
              <table className="usage-table">
                <thead>
                  <tr>
                    <th>服务商</th>
                    <th>槽位</th>
                    <th>请求</th>
                    <th>成功</th>
                    <th>失败</th>
                    <th>限流</th>
                    <th>成功率</th>
                    <th>首字</th>
                  </tr>
                </thead>
                <tbody>
                  {healthRows.map((r) => {
                    const ok = r.requests - r.failed;
                    const rate = r.requests > 0 ? `${((ok / r.requests) * 100).toFixed(1)}%` : '—';
                    return (
                      <tr key={`${r.provider_id}-${r.key}`}>
                        <td>{r.provider_id}</td>
                        <td className="mono">
                          #{r.slot_id ?? '—'}
                          {r.slot_id != null ? ` ${r.name}` : ''}
                        </td>
                        <td className="mono">{fmtNum(r.requests)}</td>
                        <td className="mono st-ok">{fmtNum(ok)}</td>
                        <td className={`mono${r.failed > 0 ? ' st-fail' : ''}`}>{fmtNum(r.failed)}</td>
                        <td className={`mono${r.rate_limited > 0 ? ' st-rl' : ''}`}>
                          {fmtNum(r.rate_limited)}
                        </td>
                        <td className="mono">{rate}</td>
                        <td className="mono">
                          {r.avg_first_token_ms != null ? `${r.avg_first_token_ms}ms` : '—'}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      <div className="set-card">
        <div className="set-card-h">
          最近请求明细
          {drillLabel && (
            <>
              <span className="sp" />
              <span className="usage-drill">
                已下钻:{drillLabel}
                <button onClick={() => setUsageDrill(null)}>清除</button>
              </span>
            </>
          )}
        </div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <div className="usage-scroll">
            <table className="usage-table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>模型</th>
                  <th>助手</th>
                  <th>槽位</th>
                  <th>Key</th>
                  <th>状态</th>
                  <th>Token</th>
                  <th>成本</th>
                  <th>首字</th>
                </tr>
              </thead>
              <tbody>
                {usageListShown.map((e) => (
                  <tr key={e.id}>
                    <td className="mono">{fmtClock(e.created_at)}</td>
                    <td>{e.model_id}</td>
                    <td>{e.agent_name ?? '—'}</td>
                    <td className="mono">
                      #{e.slot_id}
                      {e.slot_label ? ` ${e.slot_label}` : ''}
                    </td>
                    <td className="mono">{e.key_masked ?? '—'}</td>
                    <td
                      className={
                        e.status === 'success'
                          ? 'st-ok'
                          : e.status === 'rate_limited'
                            ? 'st-rl'
                            : 'st-fail'
                      }
                    >
                      {STATUS_LABEL[e.status] ?? e.status}
                      {e.error_kind ? ` · ${KIND_LABEL[e.error_kind] ?? e.error_kind}` : ''}
                    </td>
                    <td className="mono">{e.total_tokens != null ? fmtNum(e.total_tokens) : '—'}</td>
                    <td className="mono">{fmtCost(e.cost_micro)}</td>
                    <td className="mono">{e.first_token_ms != null ? `${e.first_token_ms}ms` : '—'}</td>
                  </tr>
                ))}
                {usageListShown.length === 0 && (
                  <tr>
                    <td colSpan={9}>暂无记录</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </>
  );

  const viewByNav: Record<string, ReactElement> = {
    providers: providersView,
    memory: memoryView,
    search: searchView,
    translate: translateView,
    selection: selectionView,
    usage: usageView,
    storage: storageView,
    about: aboutView,
  };

  return (
    <div className="pg pg-settings">
      <nav className="set-nav" style={{ width: navPanel.width }} data-narrow={navPanel.narrow}>
        {NAV.map((g) => (
          <div key={g.grp}>
            <div className="grp">{g.grp}</div>
            {g.items.map((it) => {
              const Icon = it.icon;
              return (
                <div
                  key={it.id}
                  className={`it${nav === it.id ? ' on' : ''}`}
                  onClick={() => setNav(it.id)}
                >
                  <Icon size={15} />
                  {it.label}
                </div>
              );
            })}
          </div>
        ))}
      </nav>
      <ResizeHandle panel={navPanel} />

      <div className="set-content">{viewByNav[nav]}</div>
    </div>
  );
}
