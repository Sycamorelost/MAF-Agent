import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import type { AttachmentEntry } from '../../lib/ipc';
import AttachmentPreviewPane from './AttachmentPreviewPane';

const entry = (meta: Partial<AttachmentEntry['meta']> = {}): AttachmentEntry =>
  ({
    message_id: 7,
    session_id: 's1',
    session_title: '季度复盘',
    branch_id: 'main',
    created: '2026-11-01T10:00:00Z',
    meta: {
      name: 'report.pdf',
      path: 'D:\\att\\report.pdf',
      kind: 'pdf',
      size: 2048,
      extracted_text: null,
      ...meta,
    },
  }) as AttachmentEntry;

const render = (e: AttachmentEntry) =>
  renderToStaticMarkup(createElement(AttachmentPreviewPane, { entry: e }));

describe('附件导入预览栏', () => {
  it('展示文件名、来源会话、类型与解析块数', () => {
    const html = render(entry({ chunk_count: 12, summary: '季度营收与成本拆解。' }));
    expect(html).toContain('report.pdf');
    expect(html).toContain('季度复盘');
    expect(html).toContain('已入库 12 块');
    expect(html).toContain('季度营收与成本拆解。');
  });

  it('未解析条目展示原因并标记无法复用', () => {
    const html = render(entry({ chunk_count: 0, extract_note: 'PDF 为扫描件,未提取到文本' }));
    expect(html).toContain('PDF 为扫描件,未提取到文本');
    expect(html).toContain('无法复用');
  });

  it('图片附件视为无需解析', () => {
    const html = render(entry({ kind: 'image', chunk_count: null }));
    expect(html).toContain('无需解析');
    expect(html).not.toContain('无法复用');
  });

  it('无摘要时回落到提取正文前 400 字', () => {
    const html = render(entry({ extracted_text: '这是一段提取出来的正文。' }));
    expect(html).toContain('这是一段提取出来的正文。');
  });
});
