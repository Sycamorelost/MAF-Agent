//! 设置 · 用量统计视图(从 SettingsPage 抽出,§10.2.1 A 阶段)。
//!
//! 只负责组合;数据加载与窗口/维度/下钻状态在 useUsageData。
//! 成本口径:纳元(1e-9 元),展示时转换为元;未计价与覆盖率显式标注。
//! 各分区见 ./usage/ 下的 Overview / Heatmap / Groups / Health / Entries。

import type { UsageData } from './useUsageData';
import { UsageEntries } from './usage/UsageEntries';
import { UsageGroups } from './usage/UsageGroups';
import { UsageHealth } from './usage/UsageHealth';
import { UsageHeatmap } from './usage/UsageHeatmap';
import { UsageOverview } from './usage/UsageOverview';

export function UsageView({ data }: { data: UsageData }) {
  return (
    <>
      <h2>用量统计</h2>
      <p className="set-desc">
        统计本机请求的 Token 用量、失败率与首字延迟,可按模型 / 服务商 / API Key / 助手分类查看。数据仅存本地。
        成本口径:(输入 − 缓存命中) × 未命中价 + 缓存命中 × 命中价 + 输出 × 输出价;USD 刊例按落库快照汇率换算;
        未计价请求不计入总额并单独标注,思考 token 已含在输出内、不重复相加。
      </p>

      <UsageOverview data={data} />
      <UsageHeatmap data={data} />
      <UsageGroups data={data} />
      <UsageHealth data={data} />
      <UsageEntries data={data} />
    </>
  );
}
