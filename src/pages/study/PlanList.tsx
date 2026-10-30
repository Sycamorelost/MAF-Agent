import { useEffect, useMemo, useState } from 'react';
import AsideDrawer from '../../components/AsideDrawer';
import { IconGraduation, IconPlay, IconTrash } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import { useProvidersStore } from '../../stores/providers';
import type { PlanRow, StudyIntent } from '../../lib/ipc/study';

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

/* ---------------- 左抽屉:规划面板 ---------------- */

export function PlanPanel() {
  const { intent, starting, preflight, clearIntent, start } = useStudyStore();
  const current = useProvidersStore((s) => s.current);
  const templatesProviders = useProvidersStore((s) => s.templates);
  const hasKey = useProvidersStore((s) => s.hasKey);

  const [topic, setTopic] = useState('');
  const [level, setLevel] = useState('');
  const [dailyMinutes, setDailyMinutes] = useState(30);
  const [deadline, setDeadline] = useState('');
  const [lang, setLang] = useState('zh');
  const [modelSel, setModelSel] = useState('');
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const modelOptions = useMemo(() => {
    const out: { value: string; label: string }[] = [];
    for (const t of templatesProviders) {
      if (!hasKey[t.id]) continue;
      for (const m of t.models) {
        out.push({ value: `${t.id}::${m}`, label: `${t.display_name} · ${m}` });
      }
    }
    return out;
  }, [templatesProviders, hasKey]);

  const modelLabel = current
    ? `${templatesProviders.find((t) => t.id === current.providerId)?.display_name ?? current.providerId} · ${current.model}`
    : '未选择';

  const parseModel = (): { providerId: string; model: string } | null => {
    if (!modelSel) return null;
    const [providerId, model] = modelSel.split('::');
    return providerId && model ? { providerId, model } : null;
  };

  const needClarify = !!intent && intent.questions.length > 0;

  const begin = async () => {
    if (busy) return;
    if (!topic.trim()) {
      setError('请先填写学习主题');
      return;
    }
    setError(null);
    setBusy(true);
    try {
      if (!intent) {
        // 第一次点击:preflight 抽取意图(带上面板已填字段,不再追问),缺字段则内联追问
        await preflight(topic.trim(), lang, {
          level: level || null,
          dailyMinutes: dailyMinutes || null,
          deadline: deadline || null,
        });
      }
      const it = useStudyStore.getState().intent;
      if (!it) return;
      if (it.questions.length > 0) {
        // 需要澄清:面板展示追问控件,等用户补充后再次点击
        return;
      }
      // 生效值:追问答案 > 面板字段 > 意图原值
      const effLevel = answers.level || level || it.level;
      const effMinutes = Number(answers.daily_minutes) || dailyMinutes || it.daily_minutes;
      const effDeadline = answers.deadline || deadline || it.deadline;
      const merged: StudyIntent = {
        ...it,
        topic: topic.trim() || it.topic,
        level: effLevel,
        daily_minutes: effMinutes,
        deadline: effDeadline,
        missing: [],
        questions: [],
      };
      await start({
        topic: merged.topic,
        intentJson: JSON.stringify(merged),
        dailyMinutes: effMinutes || null,
        deadline: effDeadline || null,
        lang,
        modelOverride: parseModel(),
      });
      setAnswers({});
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AsideDrawer>
      <aside className="pg-aside rp-config drawer">
        <div className="pg-aside-head">
          <h2>
            <IconGraduation size={18} />
            学习规划
          </h2>
          <p>点标题栏「面板」可开合本栏;描述你想学什么,AI 生成阶段化学习计划。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">学习主题</div>
          <textarea
            className="pg-field"
            style={{ width: '100%', minHeight: 64, resize: 'vertical' }}
            placeholder="例:我想在 3 个月内入门 Rust,能读懂开源项目代码"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
          />
        </div>

        <div className="pg-sec">
          <div className="pg-lab">基础信息</div>
          <div className="pg-row">
            <span>目标水平</span>
            <select className="pg-field" value={level} onChange={(e) => setLevel(e.target.value)}>
              <option value="">自动判断</option>
              <option value="zero">零基础</option>
              <option value="beginner">初学者</option>
              <option value="intermediate">中级</option>
              <option value="advanced">高级</option>
            </select>
          </div>
          <div className="pg-row">
            <span>每天投入(分钟)</span>
            <input
              className="pg-field num"
              type="number"
              min={5}
              max={600}
              value={dailyMinutes}
              onChange={(e) => setDailyMinutes(Math.max(0, Number(e.target.value) || 0))}
            />
          </div>
          <div className="pg-row">
            <span>目标完成日</span>
            <input
              className="pg-field"
              type="date"
              value={deadline}
              onChange={(e) => setDeadline(e.target.value)}
            />
          </div>
          <div className="pg-row">
            <span>输出语言</span>
            <select className="pg-field" value={lang} onChange={(e) => setLang(e.target.value)}>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>

        {needClarify && intent && (
          <div className="pg-warn">
            <div style={{ marginBottom: 6 }}>还有几个信息想确认:</div>
            {intent.questions.map((q, i) => {
              const f = intent.missing[i] ?? intent.missing[0] ?? '';
              return (
                <div key={i} style={{ marginBottom: 8 }}>
                  <div style={{ fontSize: 'var(--font-sub)', marginBottom: 4 }}>{q}</div>
                  {f === 'level' ? (
                    <select
                      className="pg-field"
                      style={{ width: '100%' }}
                      value={answers.level ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, level: e.target.value }))}
                    >
                      <option value="">请选择…</option>
                      <option value="zero">零基础</option>
                      <option value="beginner">初学者</option>
                      <option value="intermediate">中级</option>
                      <option value="advanced">高级</option>
                    </select>
                  ) : f === 'daily_minutes' ? (
                    <input
                      className="pg-field num"
                      type="number"
                      min={5}
                      max={600}
                      style={{ width: '100%' }}
                      placeholder="如 30"
                      value={answers.daily_minutes ?? ''}
                      onChange={(e) =>
                        setAnswers((a) => ({ ...a, daily_minutes: e.target.value }))
                      }
                    />
                  ) : (
                    <input
                      className="pg-field"
                      type="date"
                      style={{ width: '100%' }}
                      value={answers.deadline ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, deadline: e.target.value }))}
                    />
                  )}
                </div>
              );
            })}
            <div style={{ fontSize: 'var(--font-sub)', marginTop: 4 }}>
              补充后点击「开始生成」;留空将按默认值处理。
            </div>
          </div>
        )}

        <div className="pg-sec">
          <div className="pg-lab">执行模型</div>
          <select
            className="pg-field"
            style={{ width: '100%' }}
            value={modelSel}
            onChange={(e) => setModelSel(e.target.value)}
            title={modelSel ? undefined : modelLabel}
          >
            <option value="">跟随当前 · {modelLabel}</option>
            {modelOptions.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>

        <div className="pg-warn">
          规划流程要连续调用模型(意图澄清 → 知识地图 → 阶段设计 → 资料推荐),比较消耗
          token,请注意预算。
        </div>

        <div className="pg-foot">
          <button className="pg-run" disabled={busy || starting} onClick={() => void begin()}>
            <IconPlay size={14} />
            {busy ? '分析中…' : needClarify ? '带答案开始生成' : intent ? '开始生成' : '生成学习计划'}
          </button>
          {needClarify && (
            <button
              className="btn-ghost"
              onClick={() => {
                clearIntent();
                setAnswers({});
              }}
            >
              重新填写
            </button>
          )}
        </div>
        {error && <div className="pg-warn">{error}</div>}
      </aside>
    </AsideDrawer>
  );
}

/* ---------------- 列表视图 ---------------- */

export function PlanList() {
  const { list, listLoaded, refreshList, openPlan, remove } = useStudyStore();
  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">学习规划</span>
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
          if (confirm(`删除学习计划「${plan.topic}」?`)) onRemove();
        }}
      >
        <IconTrash size={14} />
      </button>
    </div>
  );
}
