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
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(35, 1fr)',
          gap: 3,
          marginTop: 10,
          maxWidth: 560,
        }}
      >
        {stats.days.map((d) => {
          const on = d.activity > 0;
          return (
            <span
              key={d.date}
              title={`${d.date}${on ? ` · ${d.activity} 次学习行为` : ' · 未打卡'}`}
              style={{
                height: 10,
                borderRadius: 3,
                background: on ? 'var(--ok)' : 'var(--border-strong)',
                opacity: on ? Math.min(1, 0.55 + d.activity * 0.12) : 0.6,
              }}
            />
          );
        })}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
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
