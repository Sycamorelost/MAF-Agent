/**
 * 学术文档统一样式(D4+):与后端 `fs/docgen` 的版式口径保持一致。
 *
 * 单一数据源:后端 `docgen/style.rs` 决定 Word 里的字号/行距/缩进/三线表,
 * 这里把同一套数值翻译成 CSS,供「自包含 HTML」与「打印 PDF」两个输出通道复用,
 * 避免 Word、HTML、PDF 三种产物的排版各写一套、彼此不一致。
 *
 * 对应关系(半点 → pt,1pt = 2 半点):
 * - 正文小四 24 半点 = 12pt,1.5 倍行距,首行缩进 2 字符,两端对齐;
 * - 标题 32/28/24/24 半点 = 16/14/12/12pt,黑体;
 * - 表格五号 21 半点 = 10.5pt,学术三线表;
 * - A4 页面,上下边距 2.54cm、左右 3cm(1440/1701 twip)。
 */

/** 版式常量(与 docgen/style.rs 对齐,便于单点调整)。 */
export const ACADEMIC = {
  latin: '"Times New Roman"',
  cjk: '"SimSun", "宋体", serif',
  headingCjk: '"SimHei", "黑体", "Microsoft YaHei", sans-serif',
  mono: 'Consolas, "Courier New", monospace',
  bodyPt: 12,
  lineHeight: 1.5,
  indent: '2em',
  tablePt: 10.5,
  titlePt: 22,
  headingPt: [16, 14, 12, 12],
  pageWidth: '210mm',
  pageHeight: '297mm',
  marginTB: '25.4mm',
  marginLR: '30mm',
} as const;

/** 一条 CSS 规则:空选择器表示容器自身。 */
type Rule = [selector: string, decls: string];

const RULES: Rule[] = [
  [
    '',
    `font-family:${ACADEMIC.latin},${ACADEMIC.cjk};font-size:${ACADEMIC.bodyPt}pt;` +
      `line-height:${ACADEMIC.lineHeight};color:#000;text-align:justify;` +
      `-webkit-font-smoothing:antialiased;`,
  ],
  [
    'h1, h2, h3, h4',
    `font-family:${ACADEMIC.headingCjk};font-weight:700;color:#000;` +
      `line-height:1.35;text-align:left;text-indent:0;`,
  ],
  ['h1', `font-size:${ACADEMIC.headingPt[0]}pt;margin:0 0 12pt;`],
  ['h2', `font-size:${ACADEMIC.headingPt[1]}pt;margin:18pt 0 9pt;`],
  ['h3', `font-size:${ACADEMIC.headingPt[2]}pt;margin:12pt 0 6pt;`],
  ['h4', `font-size:${ACADEMIC.headingPt[3]}pt;margin:9pt 0 4pt;`],
  ['p', 'margin:0 0 6pt;text-indent:2em;'],
  ['p + p', 'margin-top:0;'],
  ['ul, ol', 'margin:6pt 0;padding-left:2em;'],
  ['li', 'margin:3pt 0;text-indent:0;'],
  ['table', `width:100%;border-collapse:collapse;margin:9pt 0;font-size:${ACADEMIC.tablePt}pt;`],
  ['th, td', 'border:none;padding:4pt 6pt;text-align:left;vertical-align:top;text-indent:0;'],
  ['thead th', 'border-top:1.5pt solid #000;border-bottom:0.75pt solid #000;font-weight:700;'],
  ['tbody tr:last-child td', 'border-bottom:1.5pt solid #000;'],
  ['caption, figcaption', `font-size:${ACADEMIC.tablePt}pt;text-align:center;text-indent:0;margin:4pt 0;`],
  [
    'pre',
    `font-family:${ACADEMIC.mono};font-size:9.5pt;line-height:1.45;text-align:left;text-indent:0;` +
      `background:#F7F7F8;border:0.5pt solid #D9D9D9;padding:8pt 10pt;margin:9pt 0;` +
      `white-space:pre-wrap;word-break:break-word;`,
  ],
  ['pre code', 'background:none;border:none;padding:0;font-size:inherit;'],
  [
    'code',
    `font-family:${ACADEMIC.mono};font-size:0.92em;background:#F2F3F5;` +
      `padding:0.5pt 3pt;border-radius:2pt;`,
  ],
  ['blockquote', 'margin:9pt 0;padding:4pt 0 4pt 10pt;border-left:2pt solid #BFBFBF;color:#333;'],
  ['blockquote p', 'text-indent:0;margin:0;'],
  ['img', 'max-width:100%;height:auto;display:block;margin:8pt auto;'],
  ['hr', 'border:none;border-top:0.5pt solid #BFBFBF;margin:10pt 0;'],
  ['a', 'color:#0F4D92;text-decoration:none;word-break:break-all;'],
  ['strong', 'font-weight:700;'],
];

/**
 * 生成学术样式 CSS。
 * @param scope 容器选择器;为空时规则直接作用于文档根(自包含 HTML 用)。
 */
export function academicCss(scope = ''): string {
  return RULES.map(([sel, decls]) => {
    const target = sel ? (scope ? `${scope} ${sel}` : sel) : scope || 'body';
    return `${target} { ${decls} }`;
  }).join('\n');
}

/** A4 纸张容器(屏幕预览为白纸卡片,打印时由 @page 接管页边距)。 */
export function academicPaperCss(selector: string): string {
  return `
${selector} {
  width:${ACADEMIC.pageWidth};min-height:${ACADEMIC.pageHeight};
  box-sizing:border-box;margin:0 auto;padding:${ACADEMIC.marginTB} ${ACADEMIC.marginLR};
  background:#fff;
}
@media print {
  ${selector} { width:auto;min-height:0;padding:0;margin:0; }
}`;
}
