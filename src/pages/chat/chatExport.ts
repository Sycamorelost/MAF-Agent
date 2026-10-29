import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { MessageRow } from "../../lib/ipc";

/** 解析附件 JSON 数组长度。 */
export function parseCount(json: string | null): number {
  if (!json) return 0;
  try {
    const arr = JSON.parse(json);
    return Array.isArray(arr) ? arr.length : 0;
  } catch {
    return 0;
  }
}

/** 解析来源 JSON,过滤掉无 url 的条目。 */
export function parseSources(json: string | null): { title?: string; url: string }[] {
  if (!json) return [];
  try {
    const arr = JSON.parse(json);
    return Array.isArray(arr) ? arr.filter((s) => s && typeof s.url === "string") : [];
  } catch {
    return [];
  }
}

/** 会话导出 Markdown:标题 + 元信息 + 逐条 Q/A(思考过程折叠为 details)。 */
export function buildSessionMarkdown(title: string, msgs: MessageRow[]): string {
  const q = msgs.filter((m) => m.role === "user").length;
  const a = msgs.filter((m) => m.role === "assistant").length;
  const lines: string[] = [
    `# ${title}`,
    "",
    `- 导出时间:${new Date().toLocaleString("zh-CN")}`,
    `- 消息数:${msgs.length} 条(提问 ${q} / 回答 ${a})`,
    "",
    "---",
    "",
  ];
  for (const m of msgs) {
    const time = m.created ? new Date(m.created).toLocaleString("zh-CN") : "";
    if (m.role === "user") {
      lines.push(`### 用户 · ${time}`, "", m.content, "");
      const att = parseCount(m.attachments);
      if (att > 0) lines.push(`> 附件 ×${att}`, "");
    } else if (m.role === "assistant") {
      lines.push(`### 助手${m.model_id ? `(${m.model_id})` : ""} · ${time}`, "");
      if (m.reasoning) {
        lines.push("<details>", "<summary>思考过程</summary>", "", m.reasoning, "", "</details>", "");
      }
      lines.push(m.content, "");
      const srcs = parseSources(m.sources);
      if (srcs.length > 0) {
        lines.push("来源:", ...srcs.map((s) => `- [${s.title || s.url}](${s.url})`), "");
      }
    }
  }
  return lines.join("\n");
}

/** HTML 转义(标题/思考过程/链接等非 Markdown 文本)。 */
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** 会话导出 HTML:自包含单文件(样式内联,无外部依赖),浏览器可直接打开或打印。 */
export function buildSessionHtml(title: string, msgs: MessageRow[]): string {
  const q = msgs.filter((m) => m.role === "user").length;
  const a = msgs.filter((m) => m.role === "assistant").length;
  const style = `
    :root { color-scheme: light; }
    body { margin: 0; padding: 32px 24px; background: #fff; color: #0D0D12;
      font: 14px/1.7 "Inter","PingFang SC","Microsoft YaHei","Segoe UI",sans-serif; }
    .doc { max-width: 820px; margin: 0 auto; }
    .doc-head { border-bottom: 1px solid #E8E8EE; padding-bottom: 16px; margin-bottom: 24px; }
    .doc-head h1 { margin: 0 0 8px; font-size: 22px; line-height: 1.4; }
    .doc-head .sub { margin: 0; font-size: 12px; color: #8E8EA0; }
    .msg { margin-bottom: 24px; padding-bottom: 24px; border-bottom: 1px dashed #E8E8EE; }
    .msg:last-child { border-bottom: none; }
    .msg .meta { font-size: 12px; color: #8E8EA0; margin-bottom: 8px; }
    .msg.user .md { color: #14557F; }
    .md > :first-child { margin-top: 0; }
    .md > :last-child { margin-bottom: 0; }
    .md p { margin: 0 0 8px; }
    .md h1,.md h2,.md h3,.md h4 { margin: 16px 0 8px; font-weight: 600; line-height: 1.4; }
    .md h1 { font-size: 18px; } .md h2 { font-size: 16px; } .md h3,.md h4 { font-size: 14px; }
    .md ul,.md ol { padding-left: 20px; margin: 4px 0 8px; }
    .md li + li { margin-top: 4px; }
    .md code { font-family: "JetBrains Mono","Consolas",monospace; font-size: 13px;
      background: rgba(0,0,0,.06); padding: 2px 4px; border-radius: 4px; }
    .md pre { background: #141416; color: #D6DCE5; padding: 12px 16px; border-radius: 10px;
      overflow-x: auto; font-size: 13px; line-height: 1.6; }
    .md pre code { background: transparent; padding: 0; }
    .md blockquote { border-left: 4px solid #1B6CA8; padding-left: 12px; margin: 8px 0; color: #565869; }
    .md table { border-collapse: collapse; margin: 8px 0; font-size: 13px; }
    .md th,.md td { border: 1px solid #E8E8EE; padding: 4px 8px; }
    .md th { background: #F7F7F9; font-weight: 600; }
    .md hr { border: none; border-top: 1px solid #E8E8EE; margin: 16px 0; }
    .md a { color: #1B6CA8; }
    .md img { max-width: 100%; border-radius: 10px; }
    .reasoning { margin-bottom: 8px; }
    .reasoning summary { font-size: 12px; color: #8E8EA0; cursor: pointer; }
    .reasoning pre { white-space: pre-wrap; word-break: break-word; font-size: 12px;
      color: #565869; background: #F7F7F9; padding: 8px 12px; border-radius: 8px; margin: 8px 0 0; }
    .sources { margin-top: 8px; font-size: 12px; color: #565869; }
    .sources ul { margin: 4px 0 0; padding-left: 20px; }
  `;

  const sections = msgs.map((m) => {
    const time = m.created ? new Date(m.created).toLocaleString("zh-CN") : "";
    const who = m.role === "user" ? "用户" : `助手${m.model_id ? `(${m.model_id})` : ""}`;
    const body = renderToStaticMarkup(
      createElement(ReactMarkdown, { remarkPlugins: [remarkGfm] }, m.content),
    );
    const reasoning = m.reasoning
      ? `<details class="reasoning"><summary>思考过程</summary><pre>${escapeHtml(m.reasoning)}</pre></details>`
      : "";
    const srcs = parseSources(m.sources);
    const sources =
      srcs.length > 0
        ? `<div class="sources"><b>来源</b><ul>${srcs
            .map((s) => `<li><a href="${escapeHtml(s.url)}">${escapeHtml(s.title || s.url)}</a></li>`)
            .join("")}</ul></div>`
        : "";
    return `<section class="msg ${m.role}"><div class="meta">${escapeHtml(who)}${time ? ` · ${time}` : ""}</div>${reasoning}<div class="md">${body}</div>${sources}</section>`;
  });

  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
<style>${style}</style>
</head>
<body>
<div class="doc">
<header class="doc-head">
<h1>${escapeHtml(title)}</h1>
<p class="sub">导出时间:${new Date().toLocaleString("zh-CN")} · 消息 ${msgs.length} 条(提问 ${q} / 回答 ${a})</p>
</header>
<main>
${sections.join("\n")}
</main>
</div>
</body>
</html>`;
}
