//! 设置 · 生成时长卡片:单次思考与输出的总时长上限(分钟)。
//! 作为「运行时预设」页面的一部分渲染,不再单独占一个设置分区。
//! 大工程可以把上限调大;到点只收口本次生成,已生成内容仍会保留。

import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import {
  DEFAULT_CHAT_TIMEOUT_MINUTES,
  MAX_CHAT_TIMEOUT_MINUTES,
  MIN_CHAT_TIMEOUT_MINUTES,
  TIMEOUT_PRESETS,
  clampTimeoutMinutes,
  fmtTimeoutMinutes,
} from '../../lib/chatTimeout';

export interface GenerationViewProps {
  settings: ipc.AppSettings | null;
  /** 局部设置即时保存(失败会 throw) */
  onSave: (patch: Partial<ipc.AppSettings>) => Promise<void>;
}

export function GenerationView({ settings, onSave }: GenerationViewProps) {
  const saved = clampTimeoutMinutes(
    settings?.chat_timeout_minutes ?? DEFAULT_CHAT_TIMEOUT_MINUTES,
  );
  const [val, setVal] = useState(String(saved));
  const [msg, setMsg] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // 设置被外部更新(首次加载 / 广播)时同步草稿
  useEffect(() => setVal(String(saved)), [saved]);

  const onSaveClick = async () => {
    setMsg(null);
    // 空值 / 非数字不落盘(避免误存成最小值把长任务掐断)
    const raw = Number(val);
    if (val.trim() === '' || !Number.isFinite(raw)) {
      setMsg('请输入 1–1440 之间的整数(分钟)');
      return;
    }
    setSaving(true);
    try {
      const n = clampTimeoutMinutes(raw);
      await onSave({ chat_timeout_minutes: n });
      setVal(String(n));
      setMsg(`已保存:单次生成上限 ${fmtTimeoutMinutes(n)},对下一次请求生效`);
    } catch (e) {
      setMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const failed = !!msg && msg.includes('失败');

  return (
    <div className="set-card">
      <div className="set-card-h">单次生成上限</div>
      <div className="set-card-b">
        <p className="set-desc" style={{ margin: '0 0 var(--space-3)' }}>
          单次生成的总时长上限,覆盖连接、思考、流式输出与工具调用轮次。默认 10 分钟;
          做大工程时可调大(最长 24 小时)。到点只会中断本次生成,已生成的内容会完整保留。
        </p>
        <div className="set-row">
          <label className="lbl">最长时长</label>
          <input
            className="pg-field"
            style={{ maxWidth: 120 }}
            type="number"
            min={MIN_CHAT_TIMEOUT_MINUTES}
            max={MAX_CHAT_TIMEOUT_MINUTES}
            step={1}
            value={val}
            onChange={(e) => setVal(e.target.value)}
          />
          <span className="set-hint">
            分钟({MIN_CHAT_TIMEOUT_MINUTES}–{MAX_CHAT_TIMEOUT_MINUTES};{fmtTimeoutMinutes(
              MAX_CHAT_TIMEOUT_MINUTES,
            )}
            )
          </span>
        </div>

        <div className="gen-presets">
          {TIMEOUT_PRESETS.map((n) => (
            <button
              key={n}
              type="button"
              className={`gen-chip${Number(val) === n ? ' on' : ''}`}
              onClick={() => setVal(String(n))}
              title={fmtTimeoutMinutes(n)}
            >
              {fmtTimeoutMinutes(n)}
            </button>
          ))}
        </div>

        <p className="set-desc" style={{ margin: 'var(--space-2) 0 0' }}>
          思考特别久的长任务(如大规模代码改造、长文档逐节生成)建议调到 60 分钟以上。
          调整后对下一次发送生效,不影响正在进行中的这一轮。
        </p>

        <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
          {msg && (
            <span className="set-hint" style={{ color: failed ? 'var(--err)' : 'var(--text-3)' }}>
              {msg}
            </span>
          )}
          <span className="sp" />
          <button className="btn-primary" onClick={onSaveClick} disabled={saving}>
            保存生成时长
          </button>
        </div>
      </div>
    </div>
  );
}
