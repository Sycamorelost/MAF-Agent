import type { TypeStat } from '../../lib/longTermMemory';
import { typeColor } from '../../lib/longTermMemory';

interface Props {
  stats: TypeStat[];
  /** 实体总数(用于条形宽度归一化) */
  total: number;
}

/** 类型分布卡:按 entityType 聚合的横向条形图。 */
export function TypeDistributionCard({ stats, total }: Props) {
  return (
    <div className="set-card">
      <div className="set-card-h">类型分布</div>
      <div className="set-card-b">
        <div className="ltm-bars">
          {stats.map((t) => (
            <div key={t.type} className="ltm-bar-row">
              <span className="ltm-dot" style={{ background: typeColor(t.type) }} />
              <span className="ltm-bar-label" title={t.type}>
                {t.type}
              </span>
              <span className="ltm-bar-track">
                <span
                  className="ltm-bar-fill"
                  style={{
                    width: `${Math.max(2, (t.count / Math.max(1, total)) * 100)}%`,
                    background: typeColor(t.type),
                  }}
                />
              </span>
              <span className="ltm-bar-num">
                {t.count} / {t.observations}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
