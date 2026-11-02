import { describe, it, expect } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { AttachmentsHeader } from './AttachmentsHeader';
import { AttachmentCard } from './AttachmentCard';
import { RejectLogModal } from './RejectLogModal';
import { ConfirmBatchModal, TextPreviewModal } from './AttachmentModals';
import type { AttachmentEntry, AttachmentRejectList } from '../../lib/ipc';

// 仅验证渲染分支(node 环境用 renderToStaticMarkup,不需要 DOM)

const entry = (over: Partial<AttachmentEntry> = {}, meta: Partial<AttachmentEntry['meta']> = {}) =>
  ({
    message_id: 1,
    session_id: 's1',
    session_title: '会话一',
    branch_id: 'main',
    created: '2026-11-01T10:00:00Z',
    meta: {
      name: 'a.pdf',
      path: 'D:\\att\\s1\\a.pdf',
      kind: 'pdf',
      size: 2048,
      extracted_text: null,
      ...meta,
    },
    ...over,
  }) as AttachmentEntry;

const header = (over: Partial<Parameters<typeof AttachmentsHeader>[0]> = {}) =>
  renderToStaticMarkup(
    createElement(AttachmentsHeader, {
      q: '',
      onQuery: () => {},
      kind: 'all',
      onKind: () => {},
      batchMode: false,
      selectedCount: 0,
      shownCount: 0,
      rejectTotal: 0,
      deleting: false,
      onToggleBatch: () => {},
      onSelectAll: () => {},
      onClearSelect: () => {},
      onDeleteSelected: () => {},
      onOpenRejects: () => {},
      ...over,
    }),
  );

describe('附件中心顶栏', () => {
  it('普通态不显示批量操作条与角标', () => {
    const html = header();
    expect(html).not.toContain('att-batch-bar');
    expect(html).not.toContain('att-badge');
    expect(html).toContain('批量管理');
  });

  it('批量态显示操作条,0 个可选项时全选按钮禁用', () => {
    const html = header({ batchMode: true, shownCount: 0 });
    expect(html).toContain('att-batch-bar');
    expect(html).toContain('已选 0 个');
    expect(html).toContain('disabled');
  });

  it('拒绝记录超过 99 条时角标折叠为 99+', () => {
    const html = header({ rejectTotal: 2000 });
    expect(html).toContain('99+');
  });
});

describe('附件卡片', () => {
  it('提取失败显示原因与重试按钮', () => {
    const html = renderToStaticMarkup(
      createElement(AttachmentCard, {
        entry: entry({}, { extract_note: 'PDF 未提取到文本' }),
        selectMode: false,
        selected: false,
        confirming: false,
        retrying: false,
        onPreview: () => {},
        onInsert: () => {},
        onLocate: () => {},
        onDelete: () => {},
        onToggleSelect: () => {},
        onRetry: () => {},
      }),
    );
    expect(html).toContain('att-warn');
    expect(html).toContain('att-retry');
    expect(html).toContain('PDF 未提取到文本');
    expect(html).toContain('att-ops');
  });

  it('图片与其他格式即使有 note 也不提供重试', () => {
    const html = renderToStaticMarkup(
      createElement(AttachmentCard, {
        entry: entry({}, { kind: 'image', extract_note: '不可预览' }),
        selectMode: false,
        selected: false,
        confirming: false,
        retrying: false,
        onPreview: () => {},
        onInsert: () => {},
        onLocate: () => {},
        onDelete: () => {},
        onToggleSelect: () => {},
        onRetry: () => {},
      }),
    );
    expect(html).not.toContain('att-retry');
    // 图片即便有 note 也不展示重试行
    expect(html).not.toContain('att-warn');
  });

  it('批量态隐藏操作行并显示勾选控件', () => {
    const html = renderToStaticMarkup(
      createElement(AttachmentCard, {
        entry: entry(),
        selectMode: true,
        selected: true,
        confirming: false,
        retrying: true,
        onPreview: () => {},
        onInsert: () => {},
        onLocate: () => {},
        onDelete: () => {},
        onToggleSelect: () => {},
        onRetry: () => {},
      }),
    );
    expect(html).toContain('att-select on');
    expect(html).not.toContain('att-ops');
    expect(html).toContain('selected');
  });

  it('超长文件名与特殊字符按原样转义渲染', () => {
    const longName = `${'很长的文件名'.repeat(40)}<script>.pdf`;
    const html = renderToStaticMarkup(
      createElement(AttachmentCard, {
        entry: entry({}, { name: longName }),
        selectMode: false,
        selected: false,
        confirming: false,
        retrying: false,
        onPreview: () => {},
        onInsert: () => {},
        onLocate: () => {},
        onDelete: () => {},
        onToggleSelect: () => {},
        onRetry: () => {},
      }),
    );
    expect(html).toContain('很长的文件名');
    expect(html).not.toContain('<script>');
  });
});

describe('拒绝记录弹窗', () => {
  const data = (n: number): AttachmentRejectList => ({
    total: n,
    items: Array.from({ length: n }, (_, i) => ({
      time: '2026-11-01 12:00:00',
      session_id: 's1',
      session_title: '会话一',
      name: `f${i}.exe`,
      reason: '为格式黑名单文件',
      source: 'import' as const,
    })),
  });

  it('无记录时展示空态说明', () => {
    const html = renderToStaticMarkup(
      createElement(RejectLogModal, {
        data: { total: 0, items: [] },
        loading: false,
        clearing: false,
        onClose: () => {},
        onRefresh: () => {},
        onClear: () => {},
      }),
    );
    expect(html).toContain('还没有拒绝记录');
    expect(html).toContain('共 0 条');
  });

  it('上千条记录一次性列出且清空按钮可用', () => {
    const html = renderToStaticMarkup(
      createElement(RejectLogModal, {
        data: data(1000),
        loading: false,
        clearing: false,
        onClose: () => {},
        onRefresh: () => {},
        onClear: () => {},
      }),
    );
    expect(html.split('att-reject-row').length - 1).toBe(1000);
    expect(html).toContain('共 1000 条');
    expect(html).toContain('清空记录');
  });

  it('加载中禁用刷新并不展示列表', () => {
    const html = renderToStaticMarkup(
      createElement(RejectLogModal, {
        data: null,
        loading: true,
        clearing: false,
        onClose: () => {},
        onRefresh: () => {},
        onClear: () => {},
      }),
    );
    expect(html).toContain('加载中');
    expect(html).not.toContain('att-reject-row');
  });
});

describe('浮层', () => {
  it('批量确认按数量展示并禁用重复点击', () => {
    const html = renderToStaticMarkup(
      createElement(ConfirmBatchModal, {
        count: 500,
        deleting: true,
        onCancel: () => {},
        onConfirm: () => {},
      }),
    );
    expect(html).toContain('500');
    expect(html).toContain('清理中');
    expect(html).toContain('disabled');
  });

  it('文本预览渲染完整长文本', () => {
    const text = '正文段落。'.repeat(5000);
    const html = renderToStaticMarkup(
      createElement(TextPreviewModal, { name: 'long.txt', text, onClose: () => {} }),
    );
    expect(html).toContain('long.txt');
    expect(html).toContain(text);
  });
});
