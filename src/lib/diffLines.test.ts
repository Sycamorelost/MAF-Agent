import { describe, it, expect } from 'vitest';
import { diffLines } from './diffLines';

describe('diffLines', () => {
  it('完全相同的文本全部标记为 same', () => {
    const rows = diffLines('a\nb\nc', 'a\nb\nc');
    expect(rows).toHaveLength(3);
    for (const r of rows) {
      expect(r.left?.type).toBe('same');
      expect(r.right?.type).toBe('same');
    }
  });

  it('旧文本为空时全部为 add', () => {
    const rows = diffLines('', 'x\ny');
    // ''.split('\n') 为 [''],与 'x' 不同 → 1 行 del + 2 行 add
    const adds = rows.filter((r) => r.right?.type === 'add');
    expect(adds.map((r) => r.right?.text)).toEqual(['x', 'y']);
  });

  it('新文本为空时旧行全部删除', () => {
    const rows = diffLines('x\ny', '');
    const dels = rows.filter((r) => r.left?.type === 'del');
    expect(dels.map((r) => r.left?.text)).toEqual(['x', 'y']);
  });

  it('中间一行修改产生 del + add,前后相同行保持 same', () => {
    const rows = diffLines('a\nb\nc', 'a\nB\nc');
    expect(rows[0].left?.type).toBe('same');
    expect(rows[rows.length - 1].left?.type).toBe('same');
    const del = rows.find((r) => r.left?.type === 'del');
    const add = rows.find((r) => r.right?.type === 'add');
    expect(del?.left?.text).toBe('b');
    expect(add?.right?.text).toBe('B');
  });

  it('末尾追加行只产生 add', () => {
    const rows = diffLines('a', 'a\nb\nc');
    expect(rows).toHaveLength(3);
    expect(rows[1]).toEqual({ left: null, right: { text: 'b', type: 'add' } });
    expect(rows[2]).toEqual({ left: null, right: { text: 'c', type: 'add' } });
  });

  it('不变式:左侧单元格数 = 旧行数,右侧单元格数 = 新行数', () => {
    const oldText = 'l1\nl2\nl3\nl4';
    const newText = 'l1\nX\nl3\nY\nZ';
    const rows = diffLines(oldText, newText);
    expect(rows.filter((r) => r.left !== null)).toHaveLength(4);
    expect(rows.filter((r) => r.right !== null)).toHaveLength(5);
    // 同序性:same 行成对出现且文本一致
    for (const r of rows) {
      if (r.left?.type === 'same') {
        expect(r.right?.type).toBe('same');
        expect(r.left.text).toBe(r.right?.text);
      }
    }
  });
});
