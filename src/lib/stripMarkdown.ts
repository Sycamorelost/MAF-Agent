/**
 * 去掉常见 Markdown 标记,保留自然语言正文与换行。
 * 用途:
 *  1) 消息「引用」条的单行摘要(≤120 字),避免把 > / # / 围栏语法塞进输入框;
 *  2) 消息「翻译」只发送纯文本,减少 Markdown 标记对译文质量的干扰。
 * 说明:代码围栏只去围栏行、保留代码内容(翻译时由调用方自行决定是否截断)。
 */
export function stripMarkdown(input: string): string {
  let out = input.replace(/\r\n/g, '\n');
  // 代码围栏:去掉 ``` 行,内容保留
  out = out.replace(/^\s*```[^\n]*\n/gm, '').replace(/^\s*```\s*$/gm, '');
  // 图片 / 链接:保留可读文字
  out = out.replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1');
  out = out.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');
  // 表格:去掉纯分隔行,其余竖线换成空格
  out = out.replace(/^\s*\|?[\s:|-]+\|\s*$/gm, '');
  out = out.replace(/^\s*\|/gm, '').replace(/\|\s*$/gm, '').replace(/\|/g, ' ');
  // 引用符 / 标题 / 列表标记
  out = out.replace(/^\s*>\s?/gm, '');
  out = out.replace(/^\s*#{1,6}\s+/gm, '');
  out = out.replace(/^\s*[-*+]\s+/gm, '');
  // 行内码 / 强调
  out = out.replace(/`+/g, '');
  out = out.replace(/\*\*([^*]+)\*\*/g, '$1');
  out = out.replace(/\*([^*]+)\*/g, '$1');
  out = out.replace(/~~([^~]+)~~/g, '$1');
  // 压缩多余空行
  out = out.replace(/\n{3,}/g, '\n\n').trim();
  return out;
}
