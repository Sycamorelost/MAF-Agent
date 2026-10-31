/**
 * 学习中心左抽屉规划面板:学习主题 + 基础信息 + 意图澄清追问 + 执行模型。
 */
import { useEffect, useMemo, useState } from 'react';
import AsideDrawer from '../../components/AsideDrawer';
import { IconGraduation, IconPlay } from '../../components/icons';
import { useStudyStore } from '../../stores/study';
import { useProvidersStore } from '../../stores/providers';
import type { StudyIntent } from '../../lib/ipc/study';

/** 创建入口占位文案多示例轮换:暗示全学科支持(学科由 AI 识别,不设下拉)。 */
const TOPIC_PLACEHOLDERS = [
  '例:三个月学完高中数学必修一',
  '例:通过英语四级',
  '例:入门 Rust,能读懂开源项目代码',
  '例:备战中考物理',
  '例:提升议论文写作',
];

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
  const [dailyTasks, setDailyTasks] = useState(true);
  const [fullContent, setFullContent] = useState(false);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 占位文案轮换(输入框有内容时停轮换)
  const [phIdx, setPhIdx] = useState(0);
  useEffect(() => {
    if (topic) return;
    const t = window.setInterval(() => setPhIdx((i) => (i + 1) % TOPIC_PLACEHOLDERS.length), 4000);
    return () => window.clearInterval(t);
  }, [topic]);

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
        dailyTasks,
        fullContent,
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
            学习中心
          </h2>
          <p>点标题栏「面板」可开合本栏;描述你想学什么,AI 生成阶段化学习计划。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">学习主题</div>
          <textarea
            className="pg-field st-area st-area-l"
            placeholder={TOPIC_PLACEHOLDERS[phIdx]}
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
          />
        </div>

        <div className="pg-sec">
          <div className="pg-lab">基础信息</div>
          {/* 追问展开时,这几项由上方面板统一确认,避免同一字段出现两处控件 */}
          {!needClarify && (
            <>
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
            </>
          )}
          <div className="pg-row">
            <span>输出语言</span>
            <select className="pg-field" value={lang} onChange={(e) => setLang(e.target.value)}>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
          <div className="pg-row">
            <span>生成每日任务</span>
            <input
              type="checkbox"
              checked={dailyTasks}
              onChange={(e) => setDailyTasks(e.target.checked)}
              title="把阶段知识点切成每天 25-50 分钟的具体任务,含小测与复习;取消勾选则只生成阶段计划"
            />
          </div>
          <div className="pg-row">
            <span>生成完整教学内容</span>
            <input
              type="checkbox"
              checked={fullContent}
              onChange={(e) => setFullContent(e.target.checked)}
              title="按知识点逐条生成可直接阅读的自学材料(表格/引用/公式/图示),生成时间与调用次数明显增加;不勾选则先出框架,需要时再单独生成"
            />
          </div>
        </div>

        {needClarify && intent && (
          <div className="pg-warn">
            <div className="st-mb-2">还有几个信息想确认:</div>
            {intent.questions.map((q, i) => {
              const f = intent.missing[i] ?? '';
              return (
                <div key={i} className="st-mb-2">
                  <div className="st-q">{q}</div>
                  {f === 'level' ? (
                    <select
                      className="pg-field"
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
                      placeholder="如 30"
                      value={answers.daily_minutes ?? ''}
                      onChange={(e) =>
                        setAnswers((a) => ({ ...a, daily_minutes: e.target.value }))
                      }
                    />
                  ) : f === 'deadline' ? (
                    <input
                      className="pg-field"
                      type="date"
                      value={answers.deadline ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, deadline: e.target.value }))}
                    />
                  ) : (
                    <input
                      className="pg-field"
                      placeholder="请补充…"
                      value={answers[f] ?? ''}
                      onChange={(e) => setAnswers((a) => ({ ...a, [f]: e.target.value }))}
                    />
                  )}
                </div>
              );
            })}
            <div className="st-q st-mt-1">补充后点击「开始生成」;留空将按默认值处理。</div>
          </div>
        )}

        <div className="pg-sec">
          <div className="pg-lab">执行模型</div>
          <select
            className="pg-field"
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
