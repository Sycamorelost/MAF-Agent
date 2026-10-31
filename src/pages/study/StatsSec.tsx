import { useState } from 'react';
import { useStudyStore } from '../../stores/study';
import type { StudyStats } from '../../lib/ipc/study';
import { FocusTimer } from './FocusTimer';

/* ---------------- 学习统计 + 专注计时(P2) ---------------- */

export function StatsSec({ stats, onChanged }: { stats: StudyStats | null; onChanged: () => void }) {
  const { active } = useStudyStore();
  const [focusKey, setFocusKey] = useState(0);
  if (!stats || !active) return null;
  return (
    <div className="pg-sec">
      <div className="pg-lab">学习统计</div>
      <div className="pg-chips">
        <span className="pg-chip on">连续打卡 {stats.streak_days} 天</span>
        <span className="pg-chip">累计 {stats.total_days} 天</span>
        <span className="pg-chip">专注 {stats.total_minutes} 分钟</span>
        <span className="pg-chip">
          完成 {stats.done_nodes}/{stats.total_nodes}
        </span>
        <span className="pg-chip">平均掌握 {Math.round(stats.mastery_avg * 100)}%</span>
      </div>
      <div className="st-heat">
        {stats.days.map((d) => {
          const on = d.activity > 0;
          return (
            <span
              key={d.date}
              className={on ? 'on' : ''}
              title={`${d.date}${on ? ` · ${d.activity} 次学习行为` : ' · 未打卡'}`}
              /* 透明度随当日活跃数动态计算,属运行时值,保留内联 */
              style={on ? { opacity: Math.min(1, 0.55 + d.activity * 0.12) } : undefined}
            />
          );
        })}
      </div>
      <div className="st-row-wrap st-mt-3">
        <FocusTimer
          key={focusKey}
          planId={active.id}
          onLogged={() => {
            setFocusKey((k) => k + 1);
            onChanged();
          }}
        />
        <span className="pg-hint">
          打卡 = 当日有任意学习行为(学习 / 测验 / 复习 / 专注 / 深挖);专注计时随开随停,结束自动记录时长。
        </span>
      </div>
    </div>
  );
}
