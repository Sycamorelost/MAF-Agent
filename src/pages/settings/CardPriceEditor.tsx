import { useState } from 'react';
import * as ipc from '../../lib/ipc';
import { IconCheck } from '../../components/icons';
import { numOrNull, COST_TIER_OPTIONS } from './constants';

export function CardPriceEditor({
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




