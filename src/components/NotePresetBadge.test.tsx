import { describe, it, expect } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import NotePresetBadge, { isPresetNote, displayTags } from './NotePresetBadge';

// 预置知识库角标(与 attachments.ui.test.tsx 同款静态渲染验证)

describe('预置笔记判定与标签展示', () => {
  it('preset- 前缀判定为预置', () => {
    expect(isPresetNote('preset-terms-it')).toBe(true);
    expect(isPresetNote('preset-excel-formulas')).toBe(true);
    expect(isPresetNote('a1b2c3d4')).toBe(false);
    expect(isPresetNote('')).toBe(false);
  });

  it('预置笔记的标签行过滤「预置」避免与角标重复', () => {
    expect(displayTags('preset-terms-it', ['预置', '翻译', '术语表'])).toEqual(['翻译', '术语表']);
  });

  it('普通笔记标签原样保留', () => {
    expect(displayTags('a1b2c3d4', ['预置', '学习'])).toEqual(['预置', '学习']);
  });
});

describe('预置角标渲染', () => {
  it('预置笔记渲染角标', () => {
    const html = renderToStaticMarkup(createElement(NotePresetBadge, { id: 'preset-terms-it' }));
    expect(html).toContain('preset-badge');
    expect(html).toContain('预置');
  });

  it('普通笔记不渲染', () => {
    const html = renderToStaticMarkup(createElement(NotePresetBadge, { id: 'a1b2c3d4' }));
    expect(html).toBe('');
  });
});
