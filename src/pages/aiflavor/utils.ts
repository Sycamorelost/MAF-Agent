/** 去 AI 味 · 纯函数 & 常量 (无 React 依赖)。 */

/** 去 AI 味四个专用智能体(R3 B4;R3-D 起为页头 pill 的真实路由目标)。 */
export const PILL_AGENTS = [
  { id: 'ai-flavor-detector', label: 'AI 味检测官', desc: '解读本地统计结果，逐句说明哪里像 AI、为什么' },
  { id: 'ai-flavor-rewriter', label: '去 AI 味改写师', desc: '三档人味化改写，事实与观点零改动' },
  { id: 'academic-voice', label: '学术合规润色', desc: '学术语体提升 + AI 使用披露声明' },
  { id: 'style-anchor', label: '风格档案员', desc: '从你的旧作提炼可复用的风格锚点卡' },
];

/** 改写档位。 */
export const REWRITE_LEVELS = [
  { id: 'light', label: '轻度', desc: '去套话与冗连接词' },
  { id: 'medium', label: '中度', desc: '重排句长节奏、合并碎句' },
  { id: 'deep', label: '深度', desc: '按风格锚点人格化重写' },
];

/** 风格锚点 localStorage key。 */
export const ANCHOR_KEY = 'aiflavor_style_anchor';
/** 输入预算提示阈值(val.md §5.5:≤2 万字)。 */
export const CHAR_BUDGET = 20000;

/** 从纯文本中剥离图片占位符 `[图 N]`、markdown 图片语法 `![](...)`、HTML <img>。
 *  供 Rust 端本地 AI 味检测只吃纯文本。 */
export function stripImagePlaceholders(t: string): string {
  return t
    .replace(/\[图\s*\d+\]/g, '')
    .replace(/!\[[^\]]*\]\([^)]+\)/g, '')
    .replace(/<img[^>]*>/gi, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/** 句级切分(保留句末标点),与引擎 split_sentences 语义对齐。 */
export function splitSentences(t: string): string[] {
  const out: string[] = [];
  let cur = '';
  for (const ch of t) {
    cur += ch;
    if ('。！？!?；;\n'.includes(ch)) {
      const s = cur.trim();
      if (s) out.push(s);
      cur = '';
    }
  }
  const s = cur.trim();
  if (s) out.push(s);
  return out;
}

/** 逐句对照的一行。 */
export interface DiffPair {
  o: string;
  r: string;
  kind: 'same' | 'added' | 'removed';
}

/** 句级 LCS 对齐(C4 逐句 diff 可视化);规模过大时退化为顺序配对。 */
export function diffSentences(a: string[], b: string[]): DiffPair[] {
  const n = a.length;
  const m = b.length;
  if (n * m > 1_000_000) {
    const len = Math.max(n, m);
    const out: DiffPair[] = [];
    for (let k = 0; k < len; k++) {
      out.push({ o: a[k] ?? '', r: b[k] ?? '', kind: a[k] === b[k] ? 'same' : 'added' });
    }
    return out;
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out: DiffPair[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ o: a[i], r: b[j], kind: 'same' });
      i += 1;
      j += 1;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.push({ o: a[i], r: '', kind: 'removed' });
      i += 1;
    } else {
      out.push({ o: '', r: b[j], kind: 'added' });
      j += 1;
    }
  }
  while (i < n) out.push({ o: a[i++], r: '', kind: 'removed' });
  while (j < m) out.push({ o: '', r: b[j++], kind: 'added' });
  return out;
}

/** 提取指定 Markdown 二级节(节名精确匹配);缺节返回空。 */
export function extractSection(s: string, name: string): string {
  const re = new RegExp(`##\\s*${name}[^\\n]*\\n([\\s\\S]*?)(?=\\n##\\s|\\s*$)`);
  return (s.match(re)?.[1] ?? '').trim();
}

/** 改写师输出中的「改写后全文」节(回填输入区 / 逐句 diff)。 */
export const extractRewriteFull = (s: string) => extractSection(s, '改写后全文');
/** 学术合规输出中的「润色后文本」节。 */
export const extractPolished = (s: string) => extractSection(s, '润色后文本');
/** 风格档案员输出中的「风格锚点卡」节。 */
export const extractAnchorCard = (s: string) => extractSection(s, '风格锚点卡');
