import { describe, it, expect } from 'vitest';
import { parseModelIds } from './modelList';

describe('parseModelIds', () => {
  it('按换行切分', () => {
    expect(parseModelIds('gpt-4o\ngpt-4o-mini')).toEqual(['gpt-4o', 'gpt-4o-mini']);
  });

  it('按逗号/分号/顿号/空白切分(含中文逗号)', () => {
    expect(parseModelIds('a,b;c、d，e f')).toEqual(['a', 'b', 'c', 'd', 'e', 'f']);
  });

  it('trim 并去空', () => {
    expect(parseModelIds('  a  \n\n  , b ;')).toEqual(['a', 'b']);
  });

  it('去重且保序(保留首次出现位置)', () => {
    expect(parseModelIds('b a b c a')).toEqual(['b', 'a', 'c']);
  });

  it('空文本返回空数组', () => {
    expect(parseModelIds('')).toEqual([]);
    expect(parseModelIds('  \n,; ')).toEqual([]);
  });
});
