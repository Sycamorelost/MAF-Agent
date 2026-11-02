//! memory.md 文件内容拼装 / 解析工具(纯函数,UI 与检查器共用)。

import type { Frontmatter } from './ipc';

const KNOWN_FIELDS = [
  'conversation',
  'id',
  'created',
  'updated',
  'memory_version',
  'models_used',
];

/** 从 frontmatter + body 拼出 memory.md 完整文件内容。 */
export function renderFull(fm: Frontmatter, body: string): string {
  const lines = ['---'];
  lines.push(`conversation: "${fm.conversation}"`);
  lines.push(`id: ${fm.id}`);
  if (fm.created) lines.push(`created: ${fm.created}`);
  if (fm.updated) lines.push(`updated: ${fm.updated}`);
  if (fm.memory_version) lines.push(`memory_version: ${fm.memory_version}`);
  if (fm.models_used?.length) {
    lines.push(`models_used: [${fm.models_used.map((m) => `"${m}"`).join(', ')}]`);
  }
  for (const [k, v] of Object.entries(fm)) {
    if (!KNOWN_FIELDS.includes(k)) {
      lines.push(`${k}: ${JSON.stringify(v)}`);
    }
  }
  lines.push('---');
  return `${lines.join('\n')}\n\n${body}`;
}

/** 从完整内容解析出正文(去掉 frontmatter)。 */
export function bodyOf(full: string): string {
  if (!full.startsWith('---')) return full;
  const end = full.indexOf('\n---', 3);
  if (end < 0) return full;
  return full.slice(end + 4).replace(/^\s*\n/, '');
}
