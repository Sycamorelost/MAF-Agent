import type { ActiveResearch } from '../../stores/research';
import type * as ipc from '../../lib/ipc';

/** 导出文件名:仅保留安全字符(Windows 保留字符与控制符剔除)。 */
export function safeExportName(topic: string) {
  const base = topic.replace(/[\\/:*?"<>|\r\n]+/g, '_').trim();
  return (base || '研究报告').slice(0, 60);
}

/** 自包含 HTML 模板:内联样式、系统字体、零外链、零脚本,双击即可离线阅读打印。 */
export function buildExportHtml(title: string, bodyHtml: string): string {
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
  .hljs-keyword, .hljs-literal, .hljs-symbol, .hljs-name { color: #569CD6; }
  .hljs-built_in, .hljs-type { color: #4EC9B0; }
  .hljs-number, .hljs-class { color: #B8D7A3; }
  .hljs-string, .hljs-meta .hljs-string { color: #D69D85; }
  .hljs-comment, .hljs-quote { color: #57A64A; font-style: italic; }
  .hljs-meta, .hljs-meta .hljs-keyword, .hljs-tag { color: #9B9B9B; }
  .hljs-variable, .hljs-template-variable { color: #BD63C5; }
  .hljs-attr, .hljs-attribute, .hljs-link { color: #9CDCFE; }
  .hljs-selector-tag, .hljs-selector-id, .hljs-selector-class, .hljs-bullet { color: #D7BA7D; }
  .hljs-regexp, .hljs-template-tag { color: #9A5334; }
  .hljs-emphasis { font-style: italic; }
  .hljs-strong { font-weight: 700; }
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

export function fmtDate(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function hostOf(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** 启动前预计消耗估算(tok,粗略):决策 + 检索注入 + 报告。 */
export function estimateTokens(p: ipc.ResearchParams): number {
  if (p.mode === 'concise') {
    // 简洁研究:每轮 = 自评(小)+ 整稿重写(大)+ 1 组轻量检索注入
    const rounds = Math.max(1, Math.min(10, p.max_rounds));
    const est = 800 + rounds * (800 + 6000 + 3 * 5 * 150);
    return p.token_budget > 0 ? Math.min(est, p.token_budget) : est;
  }
  const decisionRounds =
    p.reflect === 'every' ? p.max_rounds : p.reflect === 'every3' ? Math.ceil(p.max_rounds / 3) : 1;
  const decisionTok = decisionRounds * 2500;
  const perSource = p.read_length === 'full' ? 800 : 150;
  const searchTok = p.max_rounds * p.queries_per_round * 5 * perSource;
  const reportTok = 6000;
  const est = decisionTok + searchTok + reportTok;
  return p.token_budget > 0 ? Math.min(est, p.token_budget) : est;
}

export function fmtTok(n: number) {
  return n >= 1000 ? `~${Math.round(n / 100) / 10}k tok` : `~${n} tok`;
}

/** 专项模式预计消耗:逐阶段估算(覆盖值优先,缺省跟随全局)+ 阶段摘要 + 终稿。 */
export function estimateStagedTokens(p: ipc.ResearchParams): number {
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

export interface TlNode {
  key: string;
  title: string;
  state: 'done' | 'doing';
  queries: string[];
  dd?: string;
}

export function buildTimeline(trail: ipc.ResearchEvent[], status: string): TlNode[] {
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
    // 多条观察追加展示(简洁研究:检索观察 + 上一轮稿件存档共存于同一节点)
    else if (ev.type === 'observation')
      g.observation = g.observation ? `${g.observation}\n${ev.content}` : ev.content;
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

/** 当前等待确认的检查点(status=paused 且最后一个 checkpoint 之后无 resumed)。 */
export function activeCheckpoint(active: ActiveResearch): { stage: number; next: string } | null {
  if (active.status !== 'paused') return null;
  let found: { stage: number; next: string } | null = null;
  for (const ev of active.trail) {
    if (ev.type === 'checkpoint') found = { stage: ev.stage, next: ev.next };
    else if (ev.type === 'resumed') found = null;
  }
  return found;
}

/** 方案概要 chips(取前 4 项,对齐设计稿 bp-meta)。 */
export function blueprintMeta(bp: ipc.Blueprint): string[] {
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

export interface BpDlgState {
  /** saveas=另存为 / edit=编辑用户方案 / new=新建方案 */
  kind: 'saveas' | 'edit' | 'new';
  src: ipc.Blueprint | null;
  name: string;
  desc: string;
}

/** 从报告 Markdown 中提取 TL;DR 节(截 600 字);失败则取正文开头。 */
export function extractTldr(report: string): string {
  const m = /##\s*(TL;DR|TLDR)\s*\n([\s\S]*?)(\n##\s|$)/i.exec(report);
  const body = (m?.[2] ?? report).trim();
  return body.length > 600 ? `${body.slice(0, 600)}…` : body;
}
