/**
 * 统一导出服务(D4):研究报告等页面共用 PDF 打印导出。
 *
 * PDF:全屏白底覆盖层 + 原生打印对话框(用户选「另存为 PDF」)。
 *      Tauri 2.12 未暴露 print_to_pdf 字节流,故走系统打印,跨平台最稳。
 *
 * 设计原则:ExportService 只做「内容 → 格式 → 写盘」的编排,不持有 UI 状态。
 * 版式统一来自 `docStyle`:与后端 docgen(Word/实验/论文模板)同一套数值,
 * 保证「打印 PDF」与「导出 DOCX」视觉一致(宋体/Times 小四、1.5 倍行距、
 * 首行缩进、三线表、A4 页边距)。
 */

import { academicCss, academicPaperCss } from './docStyle';
import { exportPdf } from '../ipc/export';

/** 打印覆盖层的布局样式:白底、铺满视口、可滚动。 */
const PRINT_OVERLAY_CSS = `
  position:fixed;inset:0;z-index:var(--z-print);
  background:#fff;
  overflow:auto;
`;

/** 纸张容器 + 学术排版(复用于导出 HTML / 打印 PDF 两个通道)。 */
const PRINT_PAPER_CSS = `
${academicPaperCss('.maf-print-overlay .maf-print-paper')}
${academicCss('.maf-print-overlay')}
.maf-print-overlay .doc-meta {
  text-align:center;text-indent:0;font-size:10.5pt;color:#666;margin:0 0 12pt;
}
.maf-print-overlay .doc-title {
  text-align:center;font-size:22pt;font-family:"SimHei","黑体","Microsoft YaHei",sans-serif;
  font-weight:700;margin:0 0 18pt;
}
`;

/**
 * 打印专用样式(关键):
 * 覆盖层在屏幕上用 `position:fixed`,但打印引擎会把它裁剪在可视区高度内,
 * 导致内容到第 2 页末尾就断掉。打印时改回普通文档流(`static` + `overflow:visible`),
 * 并隐藏应用本身,只输出覆盖层内容。
 */
const PRINT_CSS = `
@media print {
  body > *:not(.maf-print-overlay) { display:none !important; }
  .maf-print-overlay {
    position:static !important;
    inset:auto !important;
    overflow:visible !important;
    background:#fff !important;
  }
  .maf-print-overlay pre,
  .maf-print-overlay li { break-inside:avoid; }
  .maf-print-overlay h1,
  .maf-print-overlay h2,
  .maf-print-overlay h3 { break-after:avoid; }
  .maf-print-overlay table { break-inside:auto; }
}
@page { size:A4; margin:25.4mm 30mm; }
`;

const PRINT_STYLE_ID = 'maf-print-style';

/** 行内标记:`code`、**粗体**、链接。 */
function inlineMd(line: string): string {
  const esc = (s: string) =>
    s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  return esc(line)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
}

/** 管道表格行 → 单元格文本数组。 */
function tableCells(line: string): string[] {
  const body = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  return body.split('|').map((c) => c.trim());
}

/** 是否为 Markdown 表格分隔行(|---|---:|)。 */
function isTableDivider(line: string): boolean {
  return /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+\|?\s*$/.test(line);
}

/** 把 Markdown 字符串渲染为可打印的 HTML(自包含,零依赖)。 */
function mdToPrintHtml(md: string): string {
  const lines = md.split('\n');
  const out: string[] = [];
  let inCode = false;
  let listTag: 'ul' | 'ol' | null = null;

  const closeList = () => {
    if (listTag) {
      out.push(`</${listTag}>`);
      listTag = null;
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].replace(/\r$/, '');

    if (line.startsWith('```')) {
      closeList();
      if (inCode) out.push('</pre>');
      else out.push('<pre class="doc-code">');
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push(
        line.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'),
      );
      continue;
    }

    if (/^#{1,6}\s/.test(line)) {
      closeList();
      const level = line.match(/^#+/)![0].length;
      const text = inlineMd(line.replace(/^#+\s*/, ''));
      out.push(`<h${level}>${text}</h${level}>`);
      continue;
    }

    // 管道表格:表头 + 分隔行 + 数据行
    if (
      line.includes('|') &&
      i + 1 < lines.length &&
      isTableDivider(lines[i + 1])
    ) {
      closeList();
      const header = tableCells(line);
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i].includes('|') && lines[i].trim() !== '') {
        rows.push(tableCells(lines[i]));
        i++;
      }
      i--; // 回退一格,交给 for 的 i++
      const th = header.map((c) => `<th>${inlineMd(c)}</th>`).join('');
      const tb = rows
        .map((r) => `<tr>${r.map((c) => `<td>${inlineMd(c)}</td>`).join('')}</tr>`)
        .join('\n');
      out.push(
        `<table>\n<thead><tr>${th}</tr></thead>\n<tbody>\n${tb}\n</tbody>\n</table>`,
      );
      continue;
    }

    if (/^---+$/.test(line.trim())) {
      closeList();
      out.push('<hr>');
      continue;
    }

    const ordered = /^\d+[.)]\s/.test(line);
    if (ordered || /^[-*]\s/.test(line)) {
      const tag: 'ul' | 'ol' = ordered ? 'ol' : 'ul';
      if (listTag !== tag) {
        closeList();
        out.push(`<${tag}>`);
        listTag = tag;
      }
      const markerRe = ordered ? /^\d+[.)]\s*/ : /^[-*]\s*/;
      const text = line.replace(markerRe, '');
      out.push(`<li>${inlineMd(text)}</li>`);
      continue;
    }

    if (line.trim() === '') {
      closeList();
      continue;
    }

    closeList();
    out.push(`<p>${inlineMd(line)}</p>`);
  }
  closeList();
  if (inCode) out.push('</pre>');
  return out.join('\n');
}

/**
 * 导出为 PDF:把 Markdown 渲染为全屏白底覆盖层,唤起原生打印对话框。
 * 用户在对话框中选「另存为 PDF」即可。对话框关闭后自动清理覆盖层。
 */
export async function exportToPdf(markdown: string, title: string): Promise<void> {
  const esc = title.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const html = `
    <div class="maf-print-paper">
      <h1 class="doc-title">${esc}</h1>
      <div class="doc-meta">导出时间:${new Date().toLocaleString('zh-CN')}</div>
      ${mdToPrintHtml(markdown)}
    </div>
  `;
  const overlay = document.createElement('div');
  overlay.className = 'maf-print-overlay';
  overlay.setAttribute('style', PRINT_OVERLAY_CSS);
  overlay.innerHTML = html;
  document.body.appendChild(overlay);

  // 打印样式只在导出期间存在,避免影响正常页面渲染
  let styleEl = document.getElementById(PRINT_STYLE_ID) as HTMLStyleElement | null;
  if (!styleEl) {
    styleEl = document.createElement('style');
    styleEl.id = PRINT_STYLE_ID;
    document.head.appendChild(styleEl);
  }
  styleEl.textContent = `${PRINT_PAPER_CSS}\n${PRINT_CSS}`;

  // 等待一帧确保覆盖层已渲染,再唤起打印
  await new Promise((r) => requestAnimationFrame(() => r(null)));
  try {
    await exportPdf();
  } finally {
    // 打印对话框关闭后窗口重新获得焦点时移除覆盖层;兜底 30s 超时也移除。
    const cleanup = () => {
      overlay.remove();
      document.getElementById(PRINT_STYLE_ID)?.remove();
      window.removeEventListener('focus', cleanup);
    };
    window.addEventListener('focus', cleanup, { once: true });
    window.setTimeout(cleanup, 30000);
  }
}
