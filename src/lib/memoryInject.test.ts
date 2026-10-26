import { describe, it, expect } from 'vitest';
import {
  estimateTokens,
  extractSections,
  injectByStrategy,
  enforceBudget,
  MEMORY_BUDGET_TOKENS,
} from './memoryInject';

const BODY = [
  '# 会话摘要',
  '这是摘要内容。',
  '# 关键事实与结论',
  '事实一。',
  '# 待办 / 未决问题',
  '待办一。',
  '# 其他',
  '无关内容。',
].join('\n');

describe('estimateTokens', () => {
  it('字符数 / 2 上取整', () => {
    expect(estimateTokens('abcd')).toBe(2);
    expect(estimateTokens('abc')).toBe(2);
    expect(estimateTokens('')).toBe(0);
  });
});

describe('extractSections', () => {
  it('按标题提取指定章节(含标题行)', () => {
    const out = extractSections(BODY, ['会话摘要']);
    expect(out).toContain('# 会话摘要');
    expect(out).toContain('这是摘要内容。');
    expect(out).not.toContain('事实一。');
  });

  it('wanted 为前缀匹配:「待办」命中「待办 / 未决问题」', () => {
    const out = extractSections(BODY, ['待办']);
    expect(out).toContain('待办一。');
  });

  it('同时提取多个章节', () => {
    const out = extractSections(BODY, ['会话摘要', '关键事实与结论']);
    expect(out).toContain('这是摘要内容。');
    expect(out).toContain('事实一。');
    expect(out).not.toContain('无关内容。');
  });

  it('无匹配章节时返回空串', () => {
    expect(extractSections(BODY, ['不存在的章节'])).toBe('');
  });

  it('支持二、三级标题', () => {
    const body = '## 会话摘要\n二级内容\n### 待办\n三级内容';
    const out = extractSections(body, ['会话摘要', '待办']);
    expect(out).toContain('二级内容');
    expect(out).toContain('三级内容');
  });
});

describe('injectByStrategy', () => {
  it('none 策略不注入', () => {
    expect(injectByStrategy(BODY, 'none')).toBe('');
  });

  it('正文为空白时不注入', () => {
    expect(injectByStrategy('   \n ', 'full')).toBe('');
  });

  it('full 策略返回全文', () => {
    expect(injectByStrategy(BODY, 'full')).toBe(BODY);
  });

  it('summary 策略仅取「会话摘要」节', () => {
    const out = injectByStrategy(BODY, 'summary');
    expect(out).toContain('这是摘要内容。');
    expect(out).not.toContain('事实一。');
    expect(out).not.toContain('待办一。');
  });

  it('hybrid 策略取摘要 + 关键事实 + 待办', () => {
    const out = injectByStrategy(BODY, 'hybrid');
    expect(out).toContain('这是摘要内容。');
    expect(out).toContain('事实一。');
    expect(out).toContain('待办一。');
    expect(out).not.toContain('无关内容。');
  });

  it('摘要类策略提取不到任何章节时回退全文', () => {
    const noHeadings = '没有任何标题的正文';
    expect(injectByStrategy(noHeadings, 'summary')).toBe(noHeadings);
    expect(injectByStrategy(noHeadings, 'hybrid')).toBe(noHeadings);
  });
});

describe('enforceBudget', () => {
  it('预算内原样返回', () => {
    const text = 'a'.repeat(100);
    expect(enforceBudget(text)).toBe(text);
  });

  it('恰好在预算边界(2000 tokens = 4000 字符)不截断', () => {
    const text = 'a'.repeat(MEMORY_BUDGET_TOKENS * 2);
    expect(enforceBudget(text)).toBe(text);
  });

  it('超预算硬截断到 2 倍 token 数并追加标记', () => {
    const text = 'a'.repeat(MEMORY_BUDGET_TOKENS * 2 + 10);
    const out = enforceBudget(text);
    expect(out.startsWith('a'.repeat(MEMORY_BUDGET_TOKENS * 2))).toBe(true);
    expect(out).toContain('…(记忆过长,已截断)');
    expect(out.length).toBeLessThan(text.length + 20);
  });
});
