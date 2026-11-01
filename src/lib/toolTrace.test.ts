import { describe, it, expect } from 'vitest';
import {
  artifactsFromTraces,
  mergeArtifactLists,
  parseClosedTraces,
  parseTrace,
} from './toolTrace';

/** 构造单行 maf-tool 围栏块(与后端输出格式一致)。 */
function block(v: Record<string, unknown>): string {
  return '```maf-tool\n' + JSON.stringify(v) + '\n```\n';
}

const WRITE = { s: 'builtin', t: 'fs_write_text', ok: true, c: '{"rel":"docs/a.md"}', a: '{"path":"docs/a.md"}' };

describe('parseClosedTraces(P13 增量解析)', () => {
  it('只消费已闭合块,未闭合尾部留给下一次', () => {
    const closed = block(WRITE);
    const full = closed + '```maf-tool\n{"s":"builtin","t":"fs_read"';
    const first = parseClosedTraces(full);
    expect(first.traces).toHaveLength(1);
    expect(first.traces[0].tool).toBe('fs_write_text');
    // 正则消费到闭合 ``` 为止(结尾换行不计),未闭合块不能计入 consumed
    expect(first.consumed).toBe(closed.length - 1);

    // 从 consumed 处续解析:新增部分仍未闭合 → 不产出
    const second = parseClosedTraces(full.slice(first.consumed));
    expect(second.traces).toHaveLength(0);
    expect(second.consumed).toBe(0);
  });

  it('续写闭合后只解析新增块', () => {
    const base = block(WRITE);
    const next = block({ s: 'builtin', t: 'fs_read', ok: true, c: '正文' });
    const first = parseClosedTraces(base + '```maf-tool\n{"s":"builtin","t":"fs_re');
    expect(first.consumed).toBe(base.length - 1);

    const grown = base + next;
    // 增量游标指向 base 之后;新增的完整块从游标处一次性命中
    const rest = parseClosedTraces(grown.slice(first.consumed));
    expect(rest.traces).toHaveLength(1);
    expect(rest.traces[0].tool).toBe('fs_read');
    expect(first.consumed + rest.consumed).toBe(grown.length - 1);
  });
});

describe('artifactsFromTraces + mergeArtifactLists', () => {
  it('写文件工具产出文件条目,同路径只保留后一次', () => {
    const t1 = parseTrace(JSON.stringify(WRITE))!;
    const t2 = parseTrace(JSON.stringify({ ...WRITE, a: '{"path":"docs/b.md"}' }))!;
    const extra = artifactsFromTraces([t1, t2], 3);
    expect(extra.map((a) => a.name)).toEqual(['a.md', 'b.md']);
    // step 从 stepOffset 起递增,流式伪消息 messageId=-1
    expect(extra[0].step).toBe(3);
    expect(extra[1].step).toBe(4);
    expect(extra.every((a) => a.messageId === -1)).toBe(true);

    const base = [
      { ...extra[0], path: 'docs/a.md', name: 'a.md', step: 1 },
    ];
    const merged = mergeArtifactLists(base, extra);
    expect(merged.filter((a) => a.path === 'docs/a.md')).toHaveLength(1);
    // 后一次写覆盖前一次
    expect(merged.find((a) => a.path === 'docs/a.md')!.step).toBe(3);
  });

  it('读取类工具不产生产物', () => {
    const t = parseTrace(JSON.stringify({ s: 'builtin', t: 'fs_read', ok: true, c: '正文', a: '{"path":"a.txt"}' }))!;
    expect(artifactsFromTraces([t], 1)).toHaveLength(0);
  });
});
