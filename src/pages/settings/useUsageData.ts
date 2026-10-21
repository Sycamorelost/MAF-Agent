//! 用量统计数据源:时间窗 / 分类维度 / 下钻全部下沉 SQL,图表与明细同口径。
//!
//! 从 SettingsPage 抽出(§10.2.1 A 阶段),页面只负责展示。

import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';

export type UsageDim = 'model' | 'provider' | 'key' | 'agent';

export interface UsageData {
  dim: UsageDim;
  days: number;
  summary: ipc.UsageSummary | null;
  entries: ipc.UsageEntry[];
  hours: ipc.UsageHour[];
  health: Record<string, ipc.UsageGroup[]>;
  drill: string | null;
  recalc: ipc.UsageRecalcReport | null;
  recalcBusy: boolean;
  setDays: (d: number) => void;
  /** 切换维度时自动清除下钻(分类键不可跨维度复用) */
  setDim: (d: UsageDim) => void;
  setDrill: (key: string | null) => void;
  runRecalc: () => void;
}

/**
 * @param active 当前是否停留在「用量统计」分区;离开时不发起请求。
 */
export function useUsageData(active: boolean): UsageData {
  const [dim, setDimState] = useState<UsageDim>('model');
  const [days, setDays] = useState(30);
  const [summary, setSummary] = useState<ipc.UsageSummary | null>(null);
  const [entries, setEntries] = useState<ipc.UsageEntry[]>([]);
  const [hours, setHours] = useState<ipc.UsageHour[]>([]);
  const [health, setHealth] = useState<Record<string, ipc.UsageGroup[]>>({});
  const [drill, setDrill] = useState<string | null>(null);
  const [recalc, setRecalc] = useState<ipc.UsageRecalcReport | null>(null);
  const [recalcBusy, setRecalcBusy] = useState(false);

  const setDim = (d: UsageDim) => {
    setDrill(null);
    setDimState(d);
  };

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    (async () => {
      try {
        // 明细与图表同窗口、同维度:下钻条件交给 SQL,避免"图表近 7 天、明细全历史"
        const [sum, list, hrs] = await Promise.all([
          ipc.usageSummary(dim, days),
          ipc.usageEntries(200, days, drill ? dim : undefined, drill ?? undefined),
          ipc.usageHourly(days),
        ]);
        if (cancelled) return;
        setSummary(sum);
        setEntries(list);
        setHours(hrs);
        const ids = await ipc.listStoredProviderIds();
        const pairs = await Promise.all(
          ids.map(async (id) => [id, await ipc.usageKeyHealth(id, days).catch(() => [])] as const),
        );
        if (!cancelled) setHealth(Object.fromEntries(pairs));
      } catch {
        // 统计失败不影响设置项本身
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active, dim, days, drill]);

  const runRecalc = () => {
    if (recalcBusy) return;
    setRecalcBusy(true);
    ipc
      .usageRecalc(days)
      .then(setRecalc)
      .catch(() => setRecalc(null))
      .finally(() => setRecalcBusy(false));
  };

  return {
    dim,
    days,
    summary,
    entries,
    hours,
    health,
    drill,
    recalc,
    recalcBusy,
    setDays,
    setDim,
    setDrill,
    runRecalc,
  };
}
