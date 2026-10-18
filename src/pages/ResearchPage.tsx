import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useResearchStore } from '../stores/research';
import type { ActiveResearch } from '../stores/research';
import { TutorialList } from '../components/TutorialCard';
import { ResearchTemplateDialog } from '../components/ResearchTemplateDialog';
import { StagePipelineEditor } from '../components/StagePipelineEditor';
import { ResizeHandle } from '../components/ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
import { useProvidersStore } from '../stores/providers';
import * as ipc from '../lib/ipc';
import { sanitizeHttpUrl } from '../lib/url';
import { MemoMarkdown } from '../components/MemoMarkdown';
import {
  IconPlay,
  IconPause,
  IconStop,
  IconGlobe,
  IconArrowLeft,
  IconTrash,
  IconResearch,
  IconPlus,
  IconBrain,
  IconCheck,
  IconEdit,
  IconDoc,
  IconCopy,
  IconChevronDown,
  IconDownload,
} from '../components/icons';

const STATUS_LABEL: Record<string, string> = {
  running: '运行中',
  paused: '已暂停',
  done: '已完成',
  error: '失败',
  cancelled: '已终止',
};

const DEPTH_PRESETS = [
  { label: '快速', rounds: 4 },
  { label: '标准', rounds: 8 },
  { label: '深度', rounds: 15 },
];

const SOURCE_MODES = [
  { id: 'web', label: '全网' },
  { id: 'academic', label: '学术优先' },
  { id: 'whitelist', label: '域名白名单' },
];

/* ---------------- 报告图示与导出(v1.9.0 工作块 II) ---------------- */

/** 报告内联图示:figures/ 相对引用走 IPC 读 data URL;外链仅放行 http(s)。 */
function ReportFigureImg({ id, src, alt }: { id: string; src: string; alt: string }) {
  const m = /^figures\/([\w.-]+)$/.exec(src);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!m) return;
    let alive = true;
    ipc
      .researchFigureData(id, m[1])
      .then((u) => {
        if (alive) setUrl(u);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [id, m?.[1]]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!m) {
    // 非 figures 引用:仅放行 http(s),其余一律占位(安全边界)
    const safe = /^https:\/\//.test(src) ? src : null;
    return safe ? (
      <img src={safe} alt={alt} loading="lazy" />
    ) : (
      <span className="fig-missing">[图片不可用]</span>
    );
  }
  if (failed) return <span className="fig-missing">[图示加载失败]</span>;
  return url ? <img src={url} alt={alt} loading="lazy" /> : <span className="fig-missing">[图示加载中]</span>;
}

/** 导出文件名:仅保留安全字符(Windows 保留字符与控制符剔除)。 */
function safeExportName(topic: string) {
  const base = topic.replace(/[\\/:*?"<>|\r\n]+/g, '_').trim();
  return (base || '研究报告').slice(0, 60);
}

/** 自包含 HTML 模板:内联样式、系统字体、零外链、零脚本,双击即可离线阅读打印。 */
function buildExportHtml(title: string, bodyHtml: string): string {
  const esc = title
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc}</title>
<style>
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body {
    margin: 0; padding: 40px 24px; background: #F6F7F9;
    font-family: "Segoe UI", "Microsoft YaHei", system-ui, -apple-system, sans-serif;
    color: #1F2329; font-size: 15px; line-height: 1.75;
  }
  .page { max-width: 860px; margin: 0 auto; background: #fff; padding: 48px 56px;
    border-radius: 8px; box-shadow: 0 2px 16px rgba(31,35,41,.06); }
  h1 { font-size: 26px; line-height: 1.35; margin: 0 0 24px; }
  h1, h2, h3, h4 { font-weight: 650; color: #111; margin-bottom: .5em; }
  h2 { font-size: 20px; border-bottom: 1px solid #E5E7EB; padding-bottom: 8px; margin-top: 2em; }
  h3 { font-size: 17px; margin-top: 1.6em; }
  p { margin: .6em 0; text-align: left; }
  a { color: #0F4D92; text-decoration: none; word-break: break-all; }
  a:hover { text-decoration: underline; }
  ul, ol { padding-left: 1.6em; margin: .6em 0; }
  li { margin: .25em 0; }
  blockquote { margin: 1em 0; padding: 10px 16px; border-left: 3px solid #F0B429;
    background: #FFF9EC; border-radius: 0 6px 6px 0; color: #6B5B23; }
  code { background: #F2F3F5; padding: 2px 6px; border-radius: 4px;
    font-family: Consolas, "Courier New", monospace; font-size: .9em; }
  pre { background: #1F2329; color: #E8EAED; padding: 14px 16px; border-radius: 8px;
    overflow-x: auto; font-size: 13px; line-height: 1.6; }
  pre code { background: none; color: inherit; padding: 0; }
  .codeblock { position: relative; margin: 1em 0; }
  table { border-collapse: collapse; width: 100%; margin: 1em 0; font-size: 14px; }
  th, td { border: 1px solid #E0E2E6; padding: 8px 12px; text-align: left; vertical-align: top; }
  th { background: #F5F6F8; font-weight: 600; }
  tr:nth-child(even) td { background: #FAFBFC; }
  .mermaid-block, .echarts-block { margin: 1.2em 0; text-align: center; }
  .mermaid-block svg, .echarts-block svg { max-width: 100%; height: auto; }
  .mermaid-loading, .echarts-loading { color: #98A1B3; font-size: 13px; }
  .mermaid-error, .echarts-error { color: #B42318; font-size: 13px; text-align: left; }
  img { max-width: 100%; border-radius: 6px; }
  .fig-missing { color: #98A1B3; font-size: 13px; }
  .rp-audit { border: 1px solid #E0E2E6; border-radius: 8px; padding: 16px 20px;
    background: #FAFBFC; margin-top: 2em; }
  @media print {
    body { background: #fff; padding: 0; }
    .page { box-shadow: none; border-radius: 0; padding: 0; max-width: none; }
  }
</style>
</head>
<body>
<article class="page">
<h1>${esc}</h1>
${bodyHtml}
</article>
</body>
</html>`;
}

function fmtDate(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function hostOf(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** 启动前预计消耗估算(tok,粗略):决策 + 检索注入 + 报告。 */
function estimateTokens(p: ipc.ResearchParams): number {
  const decisionRounds =
    p.reflect === 'every' ? p.max_rounds : p.reflect === 'every3' ? Math.ceil(p.max_rounds / 3) : 1;
  const decisionTok = decisionRounds * 2500;
  const perSource = p.read_length === 'full' ? 800 : 150;
  const searchTok = p.max_rounds * p.queries_per_round * 5 * perSource;
  const reportTok = 6000;
  const est = decisionTok + searchTok + reportTok;
  return p.token_budget > 0 ? Math.min(est, p.token_budget) : est;
}

function fmtTok(n: number) {
  return n >= 1000 ? `~${Math.round(n / 100) / 10}k tok` : `~${n} tok`;
}

/** 专项模式预计消耗:逐阶段估算(覆盖值优先,缺省跟随全局)+ 阶段摘要 + 终稿。 */
function estimateStagedTokens(p: ipc.ResearchParams): number {
  let total = 6000; // 终稿报告
  for (const st of p.stages) {
    if (!st.enabled) continue;
    if (st.kind !== 'research') {
      total += 5000; // 撰写阶段
      continue;
    }
    const rounds = st.max_rounds ?? p.max_rounds;
    const qpr = st.queries_per_round ?? p.queries_per_round;
    const reflect = st.reflect ?? p.reflect;
    const perSource = (st.read_length ?? p.read_length) === 'full' ? 800 : 150;
    const decisionRounds =
      reflect === 'every' ? rounds : reflect === 'every3' ? Math.ceil(rounds / 3) : 1;
    total += decisionRounds * 2500 + rounds * qpr * 5 * perSource + 1500; // + 阶段摘要
  }
  return p.token_budget > 0 ? Math.min(total, p.token_budget) : total;
}

/* ---------------- 左侧:研究配置 ---------------- */

function ConfigPanel({
  onOpenLibrary,
  namingSignal,
}: {
  onOpenLibrary: () => void;
  namingSignal: number;
}) {
  const {
    searchCfg,
    loadSearchCfg,
    start,
    blueprints,
    loadBlueprints,
    saveBlueprint,
    pendingBlueprint,
    consumeBlueprint,
    templates,
    loadTemplates,
  } = useResearchStore();
  const current = useProvidersStore((s) => s.current);
  const templatesProviders = useProvidersStore((s) => s.templates);
  const hasKey = useProvidersStore((s) => s.hasKey);
  const nav = useNavigate();
  const asidePanel = usePanelWidth('research-aside', 340, 260, 500);

  const [topic, setTopic] = useState('');
  const [params, setParams] = useState<ipc.ResearchParams>({ ...ipc.DEFAULT_RESEARCH_PARAMS });
  const [isCustom, setIsCustom] = useState(false);
  const [modelSel, setModelSel] = useState(''); // '' = 跟随当前
  const [whitelistText, setWhitelistText] = useState('');
  const [bpNaming, setBpNaming] = useState(false);
  const [bpName, setBpName] = useState('');
  const [bpError, setBpError] = useState<string | null>(null);
  /** 当前配置来源方案名(null = 自定义配置) */
  const [blueprintName, setBlueprintName] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tplDlg, setTplDlg] = useState<string | null>(null);

  useEffect(() => {
    loadSearchCfg();
    loadBlueprints();
    loadTemplates();
  }, [loadSearchCfg, loadBlueprints, loadTemplates]);

  // 方案库「应用 / 从当前配置另存」信号:打开命名 / 应用暂存方案
  useEffect(() => {
    if (namingSignal > 0) setBpNaming(true);
  }, [namingSignal]);

  useEffect(() => {
    if (!pendingBlueprint) return;
    const bp = consumeBlueprint();
    if (bp) applyBlueprintToPanel(bp);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingBlueprint]);

  const patch = (p: Partial<ipc.ResearchParams>) => {
    setBlueprintName(null); // 手动调整后配置与方案脱钩,变为自定义配置
    setParams((s) => ({ ...s, ...p }));
  };

  const engineReady =
    !!searchCfg &&
    (searchCfg.engine === 'searxng' ? !!searchCfg.base_url : searchCfg.has_api_key);

  // 科研增强:草稿生图候选(已配 Key 且提供生图模型的服务商)
  const imageProviders = useMemo(
    () => templatesProviders.filter((t) => t.image_models.length > 0 && hasKey[t.id]),
    [templatesProviders, hasKey],
  );
  const imgModels =
    imageProviders.find((t) => t.id === params.skill_image_provider)?.image_models ?? [];

  /** 开启 AI 草稿时自动补全首个可用生图服务商/模型;再点关闭。 */
  const onToggleImageDraft = () => {
    if (!params.skill_image_draft && !params.skill_image_provider) {
      const first = imageProviders[0];
      patch({
        skill_image_draft: true,
        skill_image_provider: first?.id ?? null,
        skill_image_model: first?.image_models[0] ?? null,
      });
      return;
    }
    patch({ skill_image_draft: !params.skill_image_draft });
  };

  // 执行模型下拉:跟随当前 + 全部已配 key 的 provider×model
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

  const applyBlueprintToPanel = (bp: ipc.Blueprint) => {
    setParams({ ...ipc.DEFAULT_RESEARCH_PARAMS, ...bp.params });
    setIsCustom(![4, 8, 15].includes(bp.params.max_rounds));
    setWhitelistText(bp.params.whitelist_domains.join(', '));
    setBlueprintName(bp.name);
    setBpNaming(false);
    setBpError(null);
  };

  const onSaveBlueprint = async () => {
    const name = bpName.trim();
    if (!name) return;
    setBpError(null);
    try {
      await saveBlueprint(name, '', {
        ...params,
        whitelist_domains: parseDomains(whitelistText),
      });
      setBlueprintName(name);
      setBpName('');
      setBpNaming(false);
    } catch (e) {
      setBpError(e instanceof Error ? e.message : String(e));
    }
  };

  const parseDomains = (text: string) =>
    text
      .split(/[,,\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);

  const onStart = async () => {
    if (!topic.trim() || starting) return;
    setStarting(true);
    setError(null);
    try {
      const domains = parseDomains(whitelistText);
      const finalParams = { ...params, whitelist_domains: domains };
      let override: { providerId: string; model: string } | null = null;
      if (modelSel) {
        const [providerId, ...rest] = modelSel.split('::');
        override = { providerId, model: rest.join('::') };
      }
      await start(topic.trim(), finalParams, override, blueprintName);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };

  const est = params.mode === 'staged' ? estimateStagedTokens(params) : estimateTokens(params);

  return (
    <>
      <aside className="pg-aside rp-config" style={{ width: asidePanel.width }} data-narrow={asidePanel.narrow}>
      <div className="pg-aside-head">
        <h2>
          <IconResearch size={18} />
          研究配置
        </h2>
        <p>所有参数可调,可保存为预设复用。</p>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究模式</div>
        <div className="mode-seg">
          <button
            className={params.mode === 'legacy' ? 'on' : ''}
            onClick={() => patch({ mode: 'legacy' })}
          >
            快速研究
          </button>
          <button
            className={params.mode === 'staged' ? 'on' : ''}
            onClick={() =>
              patch(
                params.stages.length > 0
                  ? { mode: 'staged' }
                  : { mode: 'staged', stages: ipc.builtinStages() },
              )
            }
          >
            专项研究
          </button>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-hint">
            专项模式按「阶段流水线」执行:每阶段独立参数与约束,前一阶段结论自动注入后续阶段。
          </div>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究主题</div>
        <textarea
          className="pg-area"
          rows={3}
          placeholder="想研究什么?例如:2026 年端侧小模型推理优化方案对比"
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
        />
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究深度</div>
        <div className="pg-seg joined">
          {DEPTH_PRESETS.map((p) => (
            <button
              key={p.rounds}
              className={!isCustom && params.max_rounds === p.rounds ? 'on' : ''}
              onClick={() => {
                setIsCustom(false);
                patch({ max_rounds: p.rounds });
              }}
            >
              {p.label}
            </button>
          ))}
          <button className={isCustom ? 'on' : ''} onClick={() => setIsCustom(true)}>
            自定义
          </button>
        </div>
        {isCustom && (
          <div className="pg-row">
            <span>轮次</span>
            <input
              className="pg-field num"
              type="number"
              min={1}
              max={25}
              value={params.max_rounds}
              onChange={(e) =>
                patch({ max_rounds: Math.max(1, Math.min(25, Number(e.target.value) || 1)) })
              }
            />
          </div>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">信息源</div>
        <div className="pg-chips">
          {SOURCE_MODES.map((m) => (
            <button
              key={m.id}
              className={`pg-chip${params.source_mode === m.id ? ' on' : ''}`}
              onClick={() => patch({ source_mode: m.id })}
            >
              {m.label}
            </button>
          ))}
        </div>
        {params.source_mode === 'whitelist' && (
          <input
            className="pg-field"
            style={{ width: '100%', marginTop: 'var(--space-2)' }}
            placeholder="域名,逗号分隔,如 arxiv.org, github.com"
            value={whitelistText}
            onChange={(e) => setWhitelistText(e.target.value)}
          />
        )}
        <div className="pg-row">
          <span>每轮查询数</span>
          <select
            className="pg-field"
            value={params.queries_per_round}
            onChange={(e) => patch({ queries_per_round: Number(e.target.value) })}
          >
            {[2, 3, 4, 5, 6, 8].map((n) => (
              <option key={n} value={n}>
                {n} 条
              </option>
            ))}
          </select>
        </div>
        <div className="pg-row">
          <span>单源阅读长度</span>
          <select
            className="pg-field"
            value={params.read_length}
            onChange={(e) => patch({ read_length: e.target.value })}
          >
            <option value="digest">摘要(240 字)</option>
            <option value="full">扩展(≤1200 字)</option>
          </select>
        </div>
        <div className="pg-row">
          <span>反思策略</span>
          <select
            className="pg-field"
            value={params.reflect}
            onChange={(e) => patch({ reflect: e.target.value })}
          >
            <option value="every">每轮反思</option>
            <option value="every3">每 3 轮</option>
            <option value="final">仅开头规划</option>
          </select>
        </div>
      </div>

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

      <div className="pg-sec">
        <div className="pg-lab">
          全局约束
          <span className="sp" />
          <span className="n">触发任一即收尾</span>
        </div>
        <div className="gc-grid">
          <div className="gc-item">
            <span className="l">总 Token 预算</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              step={10000}
              title="0 = 不限"
              value={params.token_budget}
              onChange={(e) => patch({ token_budget: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          <div className="gc-item">
            <span className="l">总检索次数 (0 不限)</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              max={200}
              value={params.max_searches}
              onChange={(e) => patch({ max_searches: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          {params.mode === 'staged' && (
            <>
              <div className="gc-item">
                <span className="l">总时长·分钟 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={600}
                  value={params.max_duration_minutes}
                  onChange={(e) =>
                    patch({ max_duration_minutes: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">总资料上限 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={500}
                  value={params.max_sources}
                  onChange={(e) =>
                    patch({ max_sources: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">查询并发 1-5</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={1}
                  max={5}
                  value={params.concurrent_queries}
                  onChange={(e) =>
                    patch({
                      concurrent_queries: Math.max(1, Math.min(5, Number(e.target.value) || 1)),
                    })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">单查询失败策略</span>
                <select
                  className="pg-field"
                  value={params.fail_policy}
                  onChange={(e) => patch({ fail_policy: e.target.value })}
                >
                  <option value="skip">跳过继续</option>
                  <option value="abort_stage">中止本阶段</option>
                  <option value="abort_research">中止研究</option>
                </select>
              </div>
              <div className="gc-item">
                <span className="l">URL 去重范围</span>
                <select
                  className="pg-field"
                  value={params.dedup_scope}
                  onChange={(e) => patch({ dedup_scope: e.target.value })}
                >
                  <option value="global">全程去重</option>
                  <option value="stage">仅阶段内</option>
                </select>
                {params.dedup_scope === 'stage' && (
                  <div className="pg-hint">分阶段去重下,资料上限仍按全局累计。</div>
                )}
              </div>
            </>
          )}
          <div className="gc-item">
            <span className="l">输出语言</span>
            <select
              className="pg-field"
              value={params.output_lang}
              onChange={(e) => patch({ output_lang: e.target.value })}
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
            <button
              type="button"
              className={`tool-chip${params.checkpoint ? ' on' : ''}`}
              onClick={() => patch({ checkpoint: !params.checkpoint })}
            >
              <IconCheck size={12} />
              检查点模式
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
              onClick={() => setTplDlg('decide')}
            >
              <IconEdit size={12} />
              全局决策提示
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
              onClick={() => setTplDlg('report')}
            >
              <IconDoc size={12} />
              终稿模板
            </button>
          </div>
        )}
        <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
          <span>搜索引擎</span>
          <span style={{ color: engineReady ? 'var(--ok)' : 'var(--warn)' }}>
            {searchCfg ? (engineReady ? '已就绪' : '未就绪') : '加载中…'}
          </span>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-hint">
            检查点:每阶段结束暂停,可查看摘要、调整下阶段配置后再继续;高亮模板表示已自定义。
          </div>
        )}
      </div>

      {params.mode === 'staged' && (
        <div className="pg-sec">
          <StagePipelineEditor stages={params.stages} onChange={(stages) => patch({ stages })} />
        </div>
      )}

      {params.mode === 'legacy' && (
        <div className="pg-sec">
          <div className="pg-lab">提示词模板</div>
          <div className="pg-chips">
            <button
              className={`pg-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              onClick={() => setTplDlg('decide')}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
            >
              <IconEdit size={13} />
              全局决策提示
            </button>
            <button
              className={`pg-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              onClick={() => setTplDlg('report')}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
            >
              <IconDoc size={13} />
              终稿模板
            </button>
          </div>
          <div className="pg-hint">高亮表示已自定义,对全部研究生效;点「恢复默认」回到内置模板。</div>
        </div>
      )}

      <div className="pg-sec">
        <div className="pg-lab">
          研究方案
          <span className="sp" />
          <span className="n">{blueprintName ?? '自定义配置'}</span>
        </div>
        <div className="pg-chips">
          {blueprints.slice(0, 5).map((bp) => (
            <span
              key={bp.name}
              className={`pg-chip${blueprintName === bp.name ? ' on' : ''}`}
              onClick={() => applyBlueprintToPanel(bp)}
              title={bp.desc || bp.name}
            >
              {bp.name}
            </span>
          ))}
          {!bpNaming && (
            <button className="pg-chip" onClick={() => setBpNaming(true)} title="将当前配置保存为方案">
              <IconPlus size={13} />
              存为方案
            </button>
          )}
          <button className="pg-chip" onClick={onOpenLibrary} title="打开方案库:全部方案 / 编辑 / 删除">
            <IconDoc size={13} />
            方案库
          </button>
        </div>
        {bpNaming && (
          <div className="pg-pair" style={{ marginTop: 'var(--space-2)' }}>
            <input
              className="pg-field"
              style={{ flex: 1 }}
              placeholder="方案名称,如:竞品周报"
              value={bpName}
              autoFocus
              onChange={(e) => setBpName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void onSaveBlueprint();
                if (e.key === 'Escape') setBpNaming(false);
              }}
            />
            <button className="btn-ghost" onClick={() => void onSaveBlueprint()} disabled={!bpName.trim()}>
              <IconCheck size={14} />
            </button>
          </div>
        )}
        {bpError && <div className="pg-hint" style={{ color: 'var(--err)' }}>{bpError}</div>}
        <div className="pg-hint">方案 = 参数 + 阶段编排 + 提示词 + 约束,一体保存复用;点方案名一键应用。</div>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">
          <IconBrain size={14} />
          科研增强
        </div>
        <div
          className="skill-row"
          onClick={() => patch({ skill_enhance: !params.skill_enhance })}
          role="presentation"
        >
          <div className="skill-row-main">
            <b>方法论注入</b>
            <span>内置研究方法论技能:检索构造 / 资料消化 / 写作纪律 / 图件契约,全部本地运行</span>
          </div>
          <div
            className={`set-switch${params.skill_enhance ? ' on' : ''}`}
            role="switch"
            aria-checked={params.skill_enhance}
          />
        </div>
        {params.skill_enhance && (
          <>
            <div
              className="skill-row"
              onClick={() => patch({ skill_audit: !params.skill_audit })}
              role="presentation"
            >
              <div className="skill-row-main">
                <b>深度审计</b>
                <span>终稿追加引用与统计审计附录,增加一次模型调用</span>
              </div>
              <div
                className={`set-switch${params.skill_audit ? ' on' : ''}`}
                role="switch"
                aria-checked={params.skill_audit}
              />
            </div>
            <div
              className="skill-row"
              onClick={onToggleImageDraft}
              role="presentation"
            >
              <div className="skill-row-main">
                <b>AI 概念示意草稿</b>
                <span>终稿追加一张概念草图并标注「AI 生成」,不构成证据</span>
              </div>
              <div
                className={`set-switch${params.skill_image_draft ? ' on' : ''}`}
                role="switch"
                aria-checked={params.skill_image_draft}
              />
            </div>
            {params.skill_image_draft && (
              <div className="pg-pair">
                <select
                  className="pg-field"
                  value={params.skill_image_provider ?? ''}
                  onChange={(e) => {
                    const id = e.target.value;
                    const models =
                      imageProviders.find((t) => t.id === id)?.image_models ?? [];
                    patch({
                      skill_image_provider: id,
                      skill_image_model: models[0] ?? null,
                    });
                  }}
                  aria-label="草稿生图服务商"
                >
                  {imageProviders.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.display_name}
                    </option>
                  ))}
                </select>
                <select
                  className="pg-field"
                  value={params.skill_image_model ?? ''}
                  onChange={(e) => patch({ skill_image_model: e.target.value })}
                  aria-label="草稿生图模型"
                >
                  {imgModels.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {params.skill_image_draft && imageProviders.length === 0 && (
              <div className="pg-hint" style={{ color: 'var(--err)' }}>
                未找到已配置密钥且提供生图模型的服务商,请先在设置页完成生图配置。
              </div>
            )}
          </>
        )}
        <div className="pg-hint">方法论注入默认开启;关闭后回退原始研究管线。</div>
      </div>

      {!engineReady && (
        <div className="pg-warn">
          尚未配置可用的搜索引擎,请先到「设置 → 联网搜索」完成配置。
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-ghost" onClick={() => nav('/settings')}>
              <IconGlobe size={14} />
              前往设置
            </button>
          </div>
        </div>
      )}
      {error && <div className="pg-warn">{error}</div>}

      <div className="pg-foot">
        <div className="pg-est">
          <span>方案</span>
          <b className="ellipsis" title={blueprintName ?? undefined}>
            {blueprintName ?? '自定义配置'}
          </b>
        </div>
        <div className="pg-est">
          <span>预计消耗</span>
          <b>
            {fmtTok(est)}
            {params.token_budget > 0 &&
            (params.mode === 'staged'
              ? estimateStagedTokens({ ...params, token_budget: 0 })
              : estimateTokens({ ...params, token_budget: 0 })) > params.token_budget
              ? ' · 触顶预算'
              : ''}
          </b>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-est">
            <span>阶段</span>
            <b>
              {params.stages.filter((s) => s.enabled).length} / {params.stages.length} 启用
            </b>
          </div>
        )}
        <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
          <button
            className="btn-ghost"
            style={{ flex: 1, justifyContent: 'center', border: '1px solid var(--border)' }}
            onClick={() => setBpNaming(true)}
            title="将当前配置保存为方案"
          >
            <IconDoc size={13} />
            存为方案
          </button>
          <button
            className="pg-run"
            style={{ flex: 2 }}
            onClick={onStart}
            disabled={
              !topic.trim() ||
              !engineReady ||
              starting ||
              (params.source_mode === 'whitelist' && parseDomains(whitelistText).length === 0) ||
              (params.mode === 'staged' && params.stages.every((s) => !s.enabled))
            }
          >
            <IconPlay size={15} />
            {starting ? '启动中…' : params.mode === 'staged' ? '开始专项研究' : '开始研究'}
          </button>
        </div>
      </div>
      <TutorialList page="research" />
      </aside>
      <ResizeHandle panel={asidePanel} />
      {tplDlg && <ResearchTemplateDialog tplKey={tplDlg} onClose={() => setTplDlg(null)} />}
    </>
  );
}

/* ---------------- 右主区:历史记录 ---------------- */

function HistoryView({ onOpenLibrary }: { onOpenLibrary: () => void }) {
  const { list, refreshList, openDetail, remove } = useResearchStore();

  useEffect(() => {
    refreshList();
  }, [refreshList]);

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">研究历史</span>
        <button onClick={onOpenLibrary} title="参数 + 阶段编排 + 提示词 + 约束,一体保存复用">
          <IconDoc size={14} />
          方案库
        </button>
      </div>
      <div className="rp-hist-list">
        {list.length === 0 && <div className="rp-empty">还没有研究记录</div>}
        {list.map((r) => (
          <div key={r.id} className="rp-hist-item" onClick={() => openDetail(r.id)}>
            <span className={`stat-dot ${r.status === 'done' ? 'ok' : r.status === 'error' ? 'err' : r.status === 'cancelled' ? 'warn' : 'info'}`} />
            <div className="rp-hist-main">
              <div className="rp-hist-topic ellipsis">{r.topic}</div>
              <div className="rp-hist-meta">
                <span className={`mode-tag ${r.mode === 'staged' ? 'staged' : 'legacy'}`}>
                  {r.mode === 'staged' ? '专项' : '快速'}
                </span>
                {r.mode === 'staged' && r.stages_total > 0 && (
                  <span className="hi-stages">
                    {r.stages_done}/{r.stages_total} 阶段
                  </span>
                )}
                {r.blueprint_name && <span className="hi-bp ellipsis">{r.blueprint_name}</span>}
                {fmtDate(r.created)} · {r.rounds}/{r.max_rounds} 轮 ·{' '}
                {STATUS_LABEL[r.status] ?? r.status}
              </div>
            </div>
            <button
              className="icon-btn"
              title="删除"
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(`删除研究「${r.topic}」?`)) void remove(r.id);
              }}
            >
              <IconTrash size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------------- 右主区:方案库 ---------------- */

/** 方案概要 chips(取前 4 项,对齐设计稿 bp-meta)。 */
function blueprintMeta(bp: ipc.Blueprint): string[] {
  const p = bp.params;
  const out: string[] = [];
  if (p.mode === 'staged') {
    out.push(`${p.stages.filter((s) => s.enabled).length} 阶段`);
    if (p.checkpoint) out.push('检查点 开');
  } else {
    out.push('快速模式');
  }
  if (p.token_budget > 0) out.push(`预算 ${Math.round(p.token_budget / 1000)}k`);
  if (p.source_mode === 'academic') out.push('学术源');
  if (p.read_length === 'full') out.push('full 阅读');
  if (p.concurrent_queries > 1) out.push(`并发 ${p.concurrent_queries}`);
  if (p.max_duration_minutes > 0) out.push(`${p.max_duration_minutes} 分钟`);
  if (p.report_prompt) out.push('自定义终稿模板');
  return out.slice(0, 4);
}

interface BpDlgState {
  /** saveas=另存为 / edit=编辑用户方案 / new=新建方案 */
  kind: 'saveas' | 'edit' | 'new';
  src: ipc.Blueprint | null;
  name: string;
  desc: string;
}

function BlueprintLibrary({
  onBack,
  onApplied,
  onSaveCurrent,
}: {
  onBack: () => void;
  onApplied: () => void;
  onSaveCurrent: () => void;
}) {
  const { blueprints, loadBlueprints, saveBlueprint, removeBlueprint, applyBlueprint } =
    useResearchStore();
  const [dlg, setDlg] = useState<BpDlgState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadBlueprints().catch(() => {});
  }, [loadBlueprints]);

  const onApply = (bp: ipc.Blueprint) => {
    applyBlueprint(bp);
    onApplied();
  };

  const onConfirmDlg = async () => {
    if (!dlg) return;
    const name = dlg.name.trim();
    if (!name) return;
    setError(null);
    try {
      if (dlg.kind === 'edit' && dlg.src) {
        // 改名 = 另存新名 + 删除旧条目
        await saveBlueprint(name, dlg.desc.trim(), dlg.src.params);
        if (name !== dlg.src.name) await removeBlueprint(dlg.src.name);
      } else if (dlg.kind === 'saveas' && dlg.src) {
        await saveBlueprint(name, dlg.desc.trim(), dlg.src.params);
      } else {
        await saveBlueprint(name, dlg.desc.trim(), { ...ipc.DEFAULT_RESEARCH_PARAMS });
      }
      setDlg(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="rp-lib">
      <div className="pg-main-head">
        <button onClick={onBack} title="返回研究历史">
          <IconArrowLeft size={14} />
          返回
        </button>
        <span className="topic">方案库</span>
        <span className="cost">参数 + 阶段编排 + 提示词 + 约束,一体保存复用</span>
        <button
          className="btn-primary"
          style={{ border: 'none' }}
          onClick={() => setDlg({ kind: 'new', src: null, name: '', desc: '' })}
        >
          <IconPlus size={14} />
          新建方案
        </button>
      </div>
      <div className="rp-lib-body">
        <div className="bp-grid">
          {blueprints.map((bp) => (
            <div key={bp.name} className="bp-card">
              <div className="bp-h">
                <span className={`ic${bp.builtin ? '' : ' user'}`}>
                  {bp.builtin ? <IconResearch size={17} /> : <IconGlobe size={17} />}
                </span>
                <span className="nm ellipsis" title={bp.name}>
                  {bp.name}
                </span>
                <span className={`tag${bp.builtin ? ' builtin' : ''}`}>
                  {bp.builtin ? '内置' : '我的方案'}
                </span>
              </div>
              <div className="bp-d">{bp.desc || '暂无描述'}</div>
              <div className="bp-meta">
                {blueprintMeta(bp).map((m) => (
                  <span key={m} className="ov-chip set">
                    {m}
                  </span>
                ))}
                {/* L2:方案不含执行模型,固定提示跟随启动时选择 */}
                <span className="ov-chip">执行模型:跟随当前</span>
              </div>
              <div className="bp-ft">
                <button
                  className="btn-brand"
                  style={{ flex: 1, justifyContent: 'center' }}
                  onClick={() => onApply(bp)}
                >
                  应用
                </button>
                <button
                  className="btn-ghost"
                  style={{ border: '1px solid var(--border)' }}
                  title="以此方案为基础另存为新方案"
                  onClick={() => setDlg({ kind: 'saveas', src: bp, name: `${bp.name} 副本`, desc: bp.desc })}
                >
                  <IconCopy size={13} />
                  另存为
                </button>
                {!bp.builtin && (
                  <>
                    <button
                      className="btn-ghost"
                      style={{ border: '1px solid var(--border)' }}
                      title="编辑名称与描述"
                      onClick={() => setDlg({ kind: 'edit', src: bp, name: bp.name, desc: bp.desc })}
                    >
                      <IconEdit size={13} />
                    </button>
                    <button
                      className="btn-ghost"
                      style={{ border: '1px solid var(--border)', color: 'var(--err)' }}
                      title="删除方案"
                      onClick={() => {
                        if (confirm(`删除方案「${bp.name}」?`)) void removeBlueprint(bp.name);
                      }}
                    >
                      <IconTrash size={13} />
                    </button>
                  </>
                )}
              </div>
            </div>
          ))}
          <div
            className="bp-card bp-new"
            onClick={onSaveCurrent}
            title="回到配置页,将当前配置保存为方案"
          >
            <IconPlus size={22} />
            <span>从当前配置另存为新方案</span>
          </div>
        </div>
        <div className="pg-hint" style={{ padding: '0 var(--pad-x) var(--space-4)' }}>
          旧版「研究预设」已自动迁移为方案(仅参数、快速模式),迁移备份保留于
          data/research/presets.json.migrated。
        </div>
      </div>

      {dlg && (
        <div className="dialog-mask" onClick={() => setDlg(null)}>
          <div className="dialog" style={{ width: 420 }} onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              {dlg.kind === 'edit' ? <IconEdit size={16} /> : <IconPlus size={16} />}
              {dlg.kind === 'edit' ? '编辑方案' : dlg.kind === 'saveas' ? '另存为方案' : '新建方案'}
            </div>
            <div className="dialog-body" style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
              <div className="ov-row">
                <span className="lbl">方案名称</span>
                <input
                  className="pg-field"
                  style={{ width: '100%' }}
                  placeholder="如:竞品周报"
                  value={dlg.name}
                  autoFocus
                  onChange={(e) => setDlg({ ...dlg, name: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void onConfirmDlg();
                    if (e.key === 'Escape') setDlg(null);
                  }}
                />
              </div>
              <div className="ov-row">
                <span className="lbl">描述</span>
                <textarea
                  className="pg-field"
                  rows={3}
                  style={{ width: '100%' }}
                  placeholder="阶段构成、适用场景、约束要点…"
                  value={dlg.desc}
                  onChange={(e) => setDlg({ ...dlg, desc: e.target.value })}
                />
              </div>
              {error && <div className="pg-hint" style={{ color: 'var(--err)' }}>{error}</div>}
              {dlg.kind !== 'edit' && (
                <div className="pg-hint">
                  {dlg.kind === 'saveas'
                    ? '参数、阶段编排、提示词与约束将完整复制;名称不可与内置方案重复。'
                    : '先以默认参数创建;应用后在配置页调整,再「存为方案」覆盖。'}
                </div>
              )}
            </div>
            <div className="tpl-foot">
              <span className="sp" />
              <button className="btn-ghost" onClick={() => setDlg(null)}>
                取消
              </button>
              <button className="btn-primary" onClick={() => void onConfirmDlg()} disabled={!dlg.name.trim()}>
                <IconCheck size={13} />
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ---------------- 报告写入会话记忆(D4.5) ---------------- */

/** 从报告 Markdown 中提取 TL;DR 节(截 600 字);失败则取正文开头。 */
function extractTldr(report: string): string {
  const m = /##\s*(TL;DR|TLDR)\s*\n([\s\S]*?)(\n##\s|$)/i.exec(report);
  const body = (m?.[2] ?? report).trim();
  return body.length > 600 ? `${body.slice(0, 600)}…` : body;
}

function WriteMemoryDialog({
  active,
  onClose,
}: {
  active: ActiveResearch;
  onClose: () => void;
}) {
  const [sessions, setSessions] = useState<ipc.SessionRow[] | null>(null);
  const [savedId, setSavedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .listSessions()
      .then(setSessions)
      .catch((e) => setError(String(e)));
  }, []);

  const writeTo = async (s: ipc.SessionRow) => {
    try {
      const tldr = extractTldr(active.report);
      const text = [
        `## 深度研究:${active.topic}`,
        ``,
        `- 时间:${fmtDate(new Date().toISOString())}`,
        `- 来源:${active.sources.length} 条`,
        `- 研究 ID:${active.id}`,
        ``,
        `### TL;DR`,
        tldr,
      ].join('\n');
      await ipc.appendMemory(s.id, text);
      setSavedId(s.id);
      setTimeout(onClose, 900);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" style={{ width: 440 }} onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">写入会话记忆</div>
        <div className="dialog-body">
          <p style={{ margin: '0 0 var(--space-3)', color: 'var(--text-3)', fontSize: 'var(--font-caption)' }}>
            将本次研究的 TL;DR 摘要追加到所选会话的 memory.md。
          </p>
          {error && <div className="pg-warn">{error}</div>}
          {!sessions && !error && <div className="rp-empty">加载会话列表…</div>}
          {sessions?.length === 0 && <div className="rp-empty">还没有会话,请先新建对话</div>}
          <div className="rp-sess-list">
            {sessions?.map((s) => (
              <button key={s.id} className="rp-sess" onClick={() => void writeTo(s)} disabled={!!savedId}>
                <span className="ellipsis">{s.title || '未命名会话'}</span>
                {savedId === s.id ? (
                  <IconCheck size={15} className="ok" />
                ) : (
                  <IconBrain size={15} />
                )}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ---------------- 右侧:运行时间线 + 来源 ---------------- */

interface TlNode {
  key: string;
  title: string;
  state: 'done' | 'doing';
  queries: string[];
  dd?: string;
}

function buildTimeline(trail: ipc.ResearchEvent[], status: string): TlNode[] {
  const order: number[] = [];
  const byRound = new Map<
    number,
    { thought?: string; queries?: string[]; sourceCount: number; observation?: string }
  >();
  for (const ev of trail) {
    if (!('round' in ev)) continue;
    let g = byRound.get(ev.round);
    if (!g) {
      g = { sourceCount: 0 };
      byRound.set(ev.round, g);
      order.push(ev.round);
    }
    if (ev.type === 'thought') g.thought = ev.content;
    else if (ev.type === 'queries') g.queries = ev.queries;
    else if (ev.type === 'sources') g.sourceCount += ev.sources.length;
    else if (ev.type === 'observation') g.observation = ev.content;
  }

  const nodes: TlNode[] = [];
  for (const r of order) {
    const g = byRound.get(r)!;
    const settled = !!g.observation || (status !== 'running' && status !== 'paused');
    if (g.thought) {
      nodes.push({ key: `t${r}`, title: `第 ${r} 轮 · 思考`, state: 'done', queries: [], dd: g.thought });
    }
    nodes.push({
      key: `q${r}`,
      title: `第 ${r} 轮 · 检索`,
      state: settled ? 'done' : 'doing',
      queries: g.queries ?? [],
      dd: g.sourceCount > 0 ? `本轮纳入 ${g.sourceCount} 条资料` : '正在检索、去重与摘要…',
    });
    if (g.observation) {
      nodes.push({ key: `o${r}`, title: `第 ${r} 轮 · 反思`, state: 'done', queries: [], dd: g.observation });
    }
  }
  return nodes;
}

/* ---------------- 专项模式:按阶段分组的时间线 ---------------- */

/** 当前等待确认的检查点(status=paused 且最后一个 checkpoint 之后无 resumed)。 */
function activeCheckpoint(active: ActiveResearch): { stage: number; next: string } | null {
  if (active.status !== 'paused') return null;
  let found: { stage: number; next: string } | null = null;
  for (const ev of active.trail) {
    if (ev.type === 'checkpoint') found = { stage: ev.stage, next: ev.next };
    else if (ev.type === 'resumed') found = null;
  }
  return found;
}

/** H1:检查点「调整下阶段」弹层 — 复用 StagePipelineEditor 编辑已完成阶段之后的剩余阶段。 */
function AdjustStagesDialog({
  researchId,
  stage,
  onClose,
  onSaved,
}: {
  researchId: string;
  /** 已完成的阶段序号(1-based),仅其后阶段可编辑 */
  stage: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [configs, setConfigs] = useState<ipc.StageConfig[] | null>(null);
  const [edited, setEdited] = useState<ipc.StageConfig[]>([]);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveErr, setSaveErr] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    ipc
      .researchGet(researchId)
      .then((d) => {
        if (!live) return;
        const cfgs = d.stages?.configs ?? [];
        setConfigs(cfgs);
        setEdited(cfgs.slice(stage));
      })
      .catch((e) => live && setLoadErr(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [researchId, stage]);

  const onConfirm = async () => {
    if (!configs || saving) return;
    setSaving(true);
    setSaveErr(null);
    try {
      // 合并:前 stage 个已完成阶段保持原样 + 编辑后的剩余阶段
      await ipc.researchUpdateStages(researchId, [...configs.slice(0, stage), ...edited]);
      onSaved();
    } catch (e) {
      setSaveErr(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  const editable = configs !== null && configs.length > 0;
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog" style={{ width: 640, maxWidth: '92vw' }} onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <IconEdit size={16} />
          调整后续阶段
        </div>
        <div className="dialog-body">
          <div className="pg-hint" style={{ marginBottom: 8 }}>
            仅编辑阶段 {stage} 之后的剩余阶段;已完成阶段不受影响。保存后立即继续执行。
          </div>
          {loadErr && <div className="pg-hint">加载失败:{loadErr}</div>}
          {!loadErr && configs === null && <div className="pg-hint">加载阶段配置中…</div>}
          {!loadErr && configs !== null && configs.length === 0 && (
            <div className="pg-hint">该研究没有阶段配置快照(旧数据),无法调整。</div>
          )}
          {editable && <StagePipelineEditor stages={edited} onChange={setEdited} />}
        </div>
        <div className="tpl-foot">
          {saveErr && <span className="tpl-foot-err">{saveErr}</span>}
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button
            className="btn-primary"
            disabled={saving || !editable}
            onClick={() => void onConfirm()}
          >
            <IconCheck size={13} />
            {saving ? '保存中…' : '保存并继续'}
          </button>
        </div>
      </div>
    </div>
  );
}

/** 检查点操作条:阶段间暂停,确认后进入下一阶段。 */
function CheckpointBar({ stage, next }: { stage: number; next: string }) {
  const stageContinue = useResearchStore((s) => s.stageContinue);
  const openDetail = useResearchStore((s) => s.openDetail);
  const activeId = useResearchStore((s) => s.active?.id);
  const [adjusting, setAdjusting] = useState(false);
  return (
    <div className="checkpoint-bar">
      <span className="cp-ic">
        <IconPause size={16} />
      </span>
      <div className="cp-t">
        <div className="t1">
          检查点:阶段 {stage} 已完成,等待确认进入「{next}」
        </div>
        <div className="t2">可先查看上方阶段摘要,或调整下阶段参数与提示词后再继续</div>
      </div>
      <button
        className="btn-ghost"
        style={{ border: '1px solid var(--border-strong)' }}
        onClick={() => setAdjusting(true)}
      >
        <IconEdit size={13} />
        调整下阶段
      </button>
      <button className="btn-primary" onClick={stageContinue}>
        <IconPlay size={13} />
        继续阶段 {stage + 1}
      </button>
      {adjusting && activeId && (
        <AdjustStagesDialog
          researchId={activeId}
          stage={stage}
          onClose={() => setAdjusting(false)}
          onSaved={() => {
            setAdjusting(false);
            // 刷新阶段快照(新增/停用立即反映到流水线展示),无论成败都继续执行
            void openDetail(activeId).finally(() => stageContinue());
          }}
        />
      )}
    </div>
  );
}

function StageTrail({
  active,
  checkpoint,
}: {
  active: ActiveResearch;
  checkpoint: { stage: number; next: string } | null;
}) {
  // 用户手动展开/收起覆盖(默认:进行中展开,其余收起)
  const [closed, setClosed] = useState<Record<string, boolean>>({});
  const groups = useMemo(() => {
    const m = new Map<number, ipc.ResearchEvent[]>();
    for (const ev of active.trail) {
      const s = 'stage' in ev && typeof ev.stage === 'number' ? ev.stage : 0;
      const arr = m.get(s);
      if (arr) arr.push(ev);
      else m.set(s, [ev]);
    }
    return m;
  }, [active.trail]);

  return (
    <>
      {active.stages.map((st, i) => {
        const no = i + 1;
        const evs = groups.get(no) ?? [];
        const nodes = buildTimeline(evs, st.status === 'running' ? active.status : 'done');
        const rounds = evs.filter((e) => e.type === 'round_start').length;
        const gap = [...evs].reverse().find((e) => e.type === 'angle_gap');
        const running = st.status === 'running';
        const open = closed[st.id] === undefined ? running : !closed[st.id];
        const isRe = st.kind === 'research';
        return (
          <Fragment key={st.id}>
            <div className={`trail-card${st.status === 'pending' ? ' wait' : ''}`}>
              <div
                className="trail-head"
                onClick={() => setClosed((m) => ({ ...m, [st.id]: open }))}
              >
                <span className={`stage-idx${isRe ? '' : ' syn'}`}>{no}</span>
                <span className="trail-title ellipsis">{st.name}</span>
                {!isRe && <span className="kind-badge sy">撰写</span>}
                {st.status === 'done' &&
                  (st.partial ? (
                    <span className="st-badge partial">
                      部分完成{rounds > 0 ? ` · ${rounds} 轮` : ''}
                    </span>
                  ) : (
                    <span className="st-badge done">
                      已完成{rounds > 0 ? ` · ${rounds} 轮` : ''}
                    </span>
                  ))}
                {running && <span className="st-badge run">进行中 · 第 {active.currentRound} 轮</span>}
                {st.status === 'pending' && <span className="st-badge wait">待执行</span>}
                {st.status === 'skipped' && <span className="st-badge skip">已跳过</span>}
                <span className="trail-hint ellipsis">
                  {st.status === 'done'
                    ? `新增资料 ${st.sources_count} 条`
                    : st.status === 'pending'
                      ? isRe
                        ? '等待前序阶段完成'
                        : '基于已有资料产出分析文本'
                      : ''}
                </span>
                <span className="arr" style={open ? { transform: 'rotate(180deg)' } : undefined}>
                  <IconChevronDown size={14} />
                </span>
              </div>
              {open && (
                <div className="trail-body">
                  {st.summary && (
                    <div className="stage-summary">
                      <div className="ss-lab">阶段摘要</div>
                      <div className="ss-txt">{st.summary}</div>
                    </div>
                  )}
                  {gap && gap.type === 'angle_gap' && (
                    <div className="stage-gap">必覆盖角度缺失:{gap.missing.join('、')}(已提醒,不阻断)</div>
                  )}
                  {nodes.length === 0 && !st.summary && (
                    <div className="rp-empty" style={{ padding: 'var(--space-3)' }}>
                      {st.status === 'skipped' ? '本阶段已跳过' : '暂无过程记录'}
                    </div>
                  )}
                  {nodes.map((n) => (
                    <div key={n.key} className={`rp-tl ${n.state}`}>
                      <div className="tt">
                        {n.title}
                        <span className={`pill${n.state === 'doing' ? ' warn' : ''}`}>
                          {n.state === 'doing' ? '运行中' : '完成'}
                        </span>
                      </div>
                      {n.queries.length > 0 && (
                        <div className="rp-queries">
                          {n.queries.map((q, qi) => (
                            <span key={qi}>{q}</span>
                          ))}
                        </div>
                      )}
                      {n.dd && <div className="dd">{n.dd}</div>}
                    </div>
                  ))}
                </div>
              )}
            </div>
            {checkpoint && checkpoint.stage === no && (
              <CheckpointBar stage={no} next={checkpoint.next} />
            )}
          </Fragment>
        );
      })}
      {active.stages.every((s) => s.status === 'pending') && active.trail.length === 0 && (
        <div className="rp-empty">正在启动研究…</div>
      )}
    </>
  );
}

function RunView({ active }: { active: ActiveResearch }) {
  const { cancel, pause, resume, closeDetail } = useResearchStore();
  const tlRef = useRef<HTMLDivElement>(null);
  const reportRef = useRef<HTMLDivElement>(null);
  const [memDlg, setMemDlg] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportHint, setExportHint] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const nodes = useMemo(() => buildTimeline(active.trail, active.status), [active.trail, active.status]);
  const running = active.status === 'running';
  const paused = active.status === 'paused';
  const checkpoint = activeCheckpoint(active);
  const canExport = active.status === 'done' && !!active.report;

  // 报告图片:figures/ 引用走 IPC 加载(稳定引用,避免 MemoMarkdown 重渲染)
  const reportComponents = useMemo(
    () => ({
      img: ({ src, alt }: { src?: string; alt?: string }) => (
        <ReportFigureImg id={active.id} src={src ?? ''} alt={alt ?? ''} />
      ),
    }),
    [active.id],
  );

  // 贴底跟随:仅在用户处于底部附近时自动滚动,上翻阅读不被打断
  const stickBottom = useRef(true);
  const onTlScroll = () => {
    const el = tlRef.current;
    if (el) stickBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  };

  useEffect(() => {
    const el = tlRef.current;
    if (el && stickBottom.current) el.scrollTo({ top: el.scrollHeight });
  }, [nodes]);

  /** 通用导出流程:保存对话框(后端原生弹窗)→ 命令调用 → 结果提示。 */
  const runExport = async (kind: 'md' | 'html') => {
    setExportOpen(false);
    if (exporting) return;
    setExporting(true);
    setExportHint(null);
    try {
      const base = safeExportName(active.topic);
      const pick = await ipc.pickSavePath(
        kind === 'md' ? '导出研究报告(Markdown)' : '导出研究报告(自包含 HTML)',
        kind === 'md' ? `${base}.md` : `${base}.html`,
        kind === 'md' ? 'Markdown 文档' : 'HTML 文档',
        kind === 'md' ? 'md' : 'html',
      );
      if (!pick) return;
      const doExport = (overwrite: boolean) =>
        kind === 'md'
          ? ipc.researchExportMarkdown(active.id, pick.token, overwrite)
          : (async () => {
              const el = reportRef.current?.querySelector('.report-md');
              const html = buildExportHtml(active.topic, el?.innerHTML ?? '');
              return ipc.researchExportHtml(active.id, html, pick.token, overwrite);
            })();
      let saved: string;
      try {
        // pick.existed:系统另存为对话框已就覆盖征得用户同意,直接透传
        saved = await doExport(pick.existed);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        // H-2 兜底:对话框返回后文件才被创建(竞态)时,应用内再确认一次
        if (msg === ipc.EXPORT_TARGET_EXISTS) {
          if (!window.confirm(`目标文件已存在:\n${pick.path}\n\n确认覆盖?`)) return;
          saved = await doExport(true);
        } else {
          throw e;
        }
      }
      setExportHint(saved ? `已导出:${saved}` : null);
    } catch (e) {
      setExportHint(`导出失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  const pct = active.maxRounds > 0 ? Math.min(100, (active.currentRound / active.maxRounds) * 100) : 0;
  const statusCls =
    active.status === 'done' ? 'ok' : active.status === 'error' ? 'err' : paused ? 'warn' : '';

  return (
    <div className="rp-run">
      <div className="rp-head">
        <button className="btn-ghost" onClick={closeDetail}>
          <IconArrowLeft size={14} />
          返回
        </button>
        <span className="topic" title={active.topic}>
          {active.topic}
        </span>
        <span className={`mode-tag ${active.mode === 'staged' ? 'staged' : 'legacy'}`}>
          {active.mode === 'staged' ? '专项' : '快速'}
        </span>
        <span className={`rp-status ${statusCls}`}>
          <i />
          {STATUS_LABEL[active.status] ?? active.status}
          {checkpoint
            ? ' · 检查点等待'
            : (running || paused) && ` · 第 ${active.currentRound} / ${active.maxRounds} 轮`}
        </span>
        {running && (
          <button className="btn-ghost" onClick={pause} title="在下一个检查点暂停">
            <IconPause size={14} />
            暂停
          </button>
        )}
        {paused && (
          <button className="btn-ghost" onClick={resume}>
            <IconPlay size={14} />
            继续
          </button>
        )}
        {(running || paused) && (
          <button className="btn-ghost" onClick={cancel}>
            <IconStop size={14} />
            终止
          </button>
        )}
        {active.status === 'done' && active.report && (
          <button className="btn-ghost" onClick={() => setMemDlg(true)} title="将报告摘要写入会话记忆">
            <IconBrain size={14} />
            写入记忆
          </button>
        )}
        {canExport && (
          <div className="rp-export-wrap">
            <button
              className="btn-ghost"
              onClick={() => setExportOpen((v) => !v)}
              disabled={exporting}
              title="导出研究报告"
            >
              <IconDownload size={14} />
              {exporting ? '导出中…' : '导出'}
              <IconChevronDown size={12} />
            </button>
            {exportOpen && (
              <>
                <div className="rp-export-mask" onClick={() => setExportOpen(false)} role="presentation" />
                <div className="rp-export-menu">
                  <button onClick={() => void runExport('md')}>
                    <IconDoc size={14} />
                    Markdown(.md)
                  </button>
                  <button onClick={() => void runExport('html')}>
                    <IconGlobe size={14} />
                    自包含 HTML(.html)
                  </button>
                  <div className="rp-export-note">
                    报告导出支持 Markdown 与自包含 HTML 两种稳定格式,不提供 PPTX/DOCX/PDF 导出。
                  </div>
                </div>
              </>
            )}
          </div>
        )}
      </div>

      <div className="rp-progress">
        <i style={{ width: `${pct}%` }} />
      </div>

      {active.error && <div className="pg-warn">{active.error}</div>}

      <div className="rp-body">
        <div className="rp-main">
          <div className="rp-timeline" ref={tlRef} onScroll={onTlScroll}>
            {active.mode === 'staged' && active.stages.length > 0 ? (
              <StageTrail active={active} checkpoint={checkpoint} />
            ) : (
              <>
                {nodes.length === 0 && <div className="rp-empty">正在启动研究…</div>}
                {nodes.map((n) => (
                  <div key={n.key} className={`rp-tl ${n.state}`}>
                    <div className="tt">
                      {n.title}
                      <span className={`pill${n.state === 'doing' ? ' warn' : ''}`}>
                        {n.state === 'doing' ? (paused ? '已暂停' : '运行中') : '完成'}
                      </span>
                    </div>
                    {n.queries.length > 0 && (
                      <div className="rp-queries">
                        {n.queries.map((q, i) => (
                          <span key={i}>{q}</span>
                        ))}
                      </div>
                    )}
                    {n.dd && <div className="dd">{n.dd}</div>}
                  </div>
                ))}
              </>
            )}
          </div>

          {active.report && (
            <div className="rp-report" ref={reportRef}>
              <div className="hd">研究报告</div>
              {exportHint && <div className="rp-export-hint">{exportHint}</div>}
              <div className="md report-md">
                <MemoMarkdown
                  content={active.report + (running ? ' ▍' : '')}
                  withCodeBlock
                  streaming={running || paused}
                  extraComponents={reportComponents}
                />
              </div>
            </div>
          )}
        </div>

        <aside className="rp-sources">
          <div className="rp-src-head">
            已纳入来源 <span>{active.sources.length}</span>
          </div>
          <div className="rp-src-list">
            {active.sources.length === 0 && <div className="rp-empty">暂无来源</div>}
            {active.sources.map((s) => {
              const safeUrl = sanitizeHttpUrl(s.url);
              if (!safeUrl) {
                return (
                  <div key={s.id} className="rp-src" title="链接不安全或格式非法,已屏蔽">
                    <div className="t">{s.title}</div>
                    <div className="u">已屏蔽</div>
                    <div className="m">
                      <span>{s.engine}</span>
                      <span>[{s.id}]</span>
                    </div>
                  </div>
                );
              }
              return (
                <a key={s.id} className="rp-src" href={safeUrl} target="_blank" rel="noreferrer">
                  <div className="t">{s.title}</div>
                  <div className="u">{hostOf(safeUrl)}</div>
                  <div className="m">
                    <span>{s.engine}</span>
                    <span>[{s.id}]</span>
                  </div>
                </a>
              );
            })}
          </div>
        </aside>
      </div>

      {memDlg && <WriteMemoryDialog active={active} onClose={() => setMemDlg(false)} />}
    </div>
  );
}

export default function ResearchPage() {
  const active = useResearchStore((s) => s.active);
  /** 主区视图:history=研究历史 / library=方案库 */
  const [mainView, setMainView] = useState<'history' | 'library'>('history');
  /** 方案库「从当前配置另存」信号:递增触发配置面板命名框 */
  const [namingSignal, setNamingSignal] = useState(0);

  const openLibrary = () => setMainView('library');
  const closeLibrary = () => setMainView('history');

  return (
    <div className="pg">
      <ConfigPanel onOpenLibrary={openLibrary} namingSignal={namingSignal} />
      <main className="pg-main">
        {active ? (
          <RunView key={active.id} active={active} />
        ) : mainView === 'library' ? (
          <BlueprintLibrary
            onBack={closeLibrary}
            onApplied={closeLibrary}
            onSaveCurrent={() => {
              setMainView('history');
              setNamingSignal((n) => n + 1);
            }}
          />
        ) : (
          <HistoryView onOpenLibrary={openLibrary} />
        )}
      </main>
    </div>
  );
}
