//! 用量统计共享格式化工具(从 UsageView 原样抽出,零行为变更)。

export const fmtNum = (n: number) => n.toLocaleString('en-US');

/** 纳元(1e-9 元)→ 人民币展示;量级越小保留位数越多,至少保留 1 位有效数字。 */
export function fmtCostNano(nano: number | null | undefined) {
  if (nano == null) return '—';
  const yuan = nano / 1e9;
  if (yuan === 0) return '¥0';
  if (yuan < 0.01) {
    // 1 纳元 = 1e-9 元:最多 9 位小数,避免小额请求被舍成 ¥0
    const digits = Math.min(9, Math.max(6, -Math.floor(Math.log10(yuan)) + 1));
    return `¥${yuan.toFixed(digits)}`;
  }
  if (yuan < 1) return `¥${yuan.toFixed(4)}`;
  return `¥${yuan.toFixed(2)}`;
}

/** 带符号的差异(用于复算报告)。 */
export function fmtDelta(nano: number | null | undefined) {
  if (nano == null) return '—';
  const s = fmtCostNano(Math.abs(nano));
  return nano === 0 ? '¥0' : `${nano > 0 ? '+' : '-'}${s}`;
}

/** Unix 秒 → MM-DD HH:mm。 */
export function fmtClock(secs: number) {
  const d = new Date(secs * 1000);
  const p = (x: number) => String(x).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function pct(v: number) {
  return `${(v * 100).toFixed(v >= 0.995 ? 0 : 1)}%`;
}

/** 计价快照的人类可读摘要(展开行展示)。 */
export function snapshotLines(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const p = (k: string) => (o[k] == null ? '—' : String(o[k]));
    return [
      `卡 id:${p('card_id')}`,
      `输入(命中):${p('input_hit')} 元/M · 输入(未命中):${p('input_miss')} 元/M`,
      `输出:${p('output')} 元/M · 生图:${p('per_image')} 元/张 · 按字符:${p('per_char')} 元/千字符`,
      `币种:${p('currency')} · 汇率:${p('fx_rate')}${o.manual ? ' · 用户手改价' : ''}`,
      `缓存口径:${p('cache')}${o.note ? ` · ${p('note')}` : ''}`,
    ];
  } catch {
    return [raw];
  }
}
