/**
 * 学习计划列表视图 + 全局共享的状态/水平文案(PlanDetail / StudyPage 复用)。
 * 左抽屉规划面板在 PlanPanel.tsx。
 */
import { useEffect, useState } from 'react';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { IconTrash } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import type { PlanRow } from '../../lib/ipc/study';

/** 本地日期时间格式化(无效值原样返回)。 */
const fmtDate = (s: string) => {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
};

export const STATUS_LABEL: Record<string, string> = {
  generating: '生成中',
  active: '进行中',
  done: '已完成',
  archived: '已归档',
  error: '失败',
  cancelled: '已终止',
};

const LEVEL_LABEL: Record<string, string> = {
  zero: '零基础',
  beginner: '初学者',
  intermediate: '中级',
  advanced: '高级',
};

export const levelText = (l: string) => LEVEL_LABEL[l] ?? '未指定';

export function PlanList() {
  const { list, listLoaded, refreshList, openPlan, remove } = useStudyStore();
  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">学习计划</span>
        <span className="pg-hint">在左侧面板描述主题,生成阶段化学习计划</span>
      </div>
      <div className="rp-hist-list">
        {listLoaded && list.length === 0 && (
          <div className="rp-empty">还没有学习计划</div>
        )}
        {list.map((p) => (
          <PlanItem key={p.id} plan={p} onOpen={() => void openPlan(p.id)} onRemove={() => void remove(p.id)} />
        ))}
      </div>
    </div>
  );
}

function PlanItem({ plan, onOpen, onRemove }: { plan: PlanRow; onOpen: () => void; onRemove: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="rp-hist-item" onClick={onOpen}>
      <span
        className={`stat-dot ${
          plan.status === 'active' || plan.status === 'done'
            ? 'ok'
            : plan.status === 'error'
              ? 'err'
              : plan.status === 'cancelled'
                ? 'warn'
                : 'info'
        }`}
      />
      <div className="rp-hist-main">
        <div className="rp-hist-topic ellipsis">{plan.topic}</div>
        <div className="rp-hist-meta">
          {plan.status === 'generating' && <span className="mode-tag staged">生成中</span>}
          {plan.repaired && <span className="mode-tag concise">降级</span>}
          <span>{levelText(plan.level)}</span>
          <span>· 每天 {plan.daily_minutes} 分钟</span>
          {plan.deadline && <span>· 截止 {plan.deadline}</span>}
          <span>· {fmtDate(plan.created)}</span>
          <span>· {STATUS_LABEL[plan.status] ?? plan.status}</span>
        </div>
      </div>
      <button
        className="icon-btn"
        title="删除"
        onClick={(e) => {
          e.stopPropagation();
          setConfirming(true);
        }}
      >
        <IconTrash size={14} />
      </button>

      {confirming && (
        // 遮罩点击会冒泡到列表项(会误触发「打开计划」),这里截断
        <div onClick={(e) => e.stopPropagation()}>
          <ConfirmDialog
            title="删除学习计划"
            message={`确定删除学习计划「${plan.topic}」?生成的教学内容、测验记录与题目会一并删除。`}
            confirmText="删除"
            danger
            onCancel={() => setConfirming(false)}
            onConfirm={() => {
              setConfirming(false);
              onRemove();
            }}
          />
        </div>
      )}
    </div>
  );
}
