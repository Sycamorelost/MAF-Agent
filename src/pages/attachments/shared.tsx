//! 附件中心共享常量与纯展示辅助(类型标签 / 图标 / 格式化 / 分组 key)。

import type { AttachmentEntry, AttachmentMeta } from '../../lib/ipc';
import {
  IconAttach,
  IconClipboardList,
  IconDoc,
  IconFileText,
  IconImage,
} from '../../components/icons';

export type Kind = AttachmentMeta['kind'];

export const KIND_LABEL: Record<Kind, string> = {
  image: '图片',
  docx: '文档',
  xlsx: '表格',
  pdf: 'PDF',
  pptx: '演示',
  epub: '电子书',
  html: '网页',
  text: '文本',
  other: '其他',
};

export const KIND_FILTERS: Array<{ key: 'all' | Kind; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'image', label: '图片' },
  { key: 'docx', label: '文档' },
  { key: 'xlsx', label: '表格' },
  { key: 'pdf', label: 'PDF' },
  { key: 'pptx', label: '演示' },
  { key: 'epub', label: '电子书' },
  { key: 'html', label: '网页' },
  { key: 'text', label: '文本' },
  { key: 'other', label: '其他' },
];

export function KindIcon({ kind }: { kind: Kind }) {
  switch (kind) {
    case 'image':
      return <IconImage size={26} strokeWidth={1.2} />;
    case 'xlsx':
      return <IconClipboardList size={26} strokeWidth={1.2} />;
    case 'text':
      return <IconFileText size={26} strokeWidth={1.2} />;
    case 'docx':
    case 'pdf':
    case 'pptx':
    case 'epub':
    case 'html':
      return <IconDoc size={26} strokeWidth={1.2} />;
    default:
      return <IconAttach size={26} strokeWidth={1.2} />;
  }
}

export const fmtSize = (n: number) =>
  n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;

export const fmtDate = (s: string) => {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString('zh-CN');
};

/** 删除确认 key:同一消息可能引用同路径两次,需组合区分。 */
export const delKey = (e: AttachmentEntry) => `${e.message_id}:${e.meta.path}`;

export interface AttachmentGroup {
  sessionId: string;
  title: string;
  items: AttachmentEntry[];
}

/** 文本提取失败且可重试(图片/其他格式本就不做文本提取)。 */
export const canRetryExtract = (e: AttachmentEntry) =>
  !!e.meta.extract_note && e.meta.kind !== 'image' && e.meta.kind !== 'other';

/** 拒绝来源中文标签 */
export const rejectSourceLabel = (source: string) => {
  switch (source) {
    case 'save':
      return '保存失败';
    default:
      return '导入拒绝';
  }
};
