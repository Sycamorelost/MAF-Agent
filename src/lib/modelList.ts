/**
 * 解析批量导入的模型 id 文本:按换行 / 逗号 / 分号 / 顿号 / 空白切分,
 * trim、去空、去重、保序。设置页「批量导入模型」与单个添加共用。
 */
export function parseModelIds(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[\s,;，、]+/)) {
    const id = raw.trim();
    if (id && !seen.has(id)) {
      seen.add(id);
      out.push(id);
    }
  }
  return out;
}
