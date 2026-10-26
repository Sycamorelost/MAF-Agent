import { describe, it, expect, beforeEach } from 'vitest';
import { useAttachmentInsertStore } from './attachmentInsert';
import type { AttachmentMeta } from '../lib/ipc';

const META: AttachmentMeta = {
  name: 'a.txt',
  path: '/tmp/a.txt',
  kind: 'text',
  size: 10,
  extracted_text: 'hi',
};

const reset = () => useAttachmentInsertStore.setState({ queue: null });

describe('attachmentInsert store', () => {
  beforeEach(reset);

  it('enqueue 后目标会话可 consume 到附件', () => {
    useAttachmentInsertStore.getState().enqueue('s1', [META]);
    expect(useAttachmentInsertStore.getState().consume('s1')).toEqual([META]);
  });

  it('consume 后队列清空,再次 consume 返回 null', () => {
    useAttachmentInsertStore.getState().enqueue('s1', [META]);
    useAttachmentInsertStore.getState().consume('s1');
    expect(useAttachmentInsertStore.getState().queue).toBeNull();
    expect(useAttachmentInsertStore.getState().consume('s1')).toBeNull();
  });

  it('非目标会话 consume 返回 null 且不清队列', () => {
    useAttachmentInsertStore.getState().enqueue('s1', [META]);
    expect(useAttachmentInsertStore.getState().consume('s2')).toBeNull();
    expect(useAttachmentInsertStore.getState().queue).not.toBeNull();
    expect(useAttachmentInsertStore.getState().consume('s1')).toEqual([META]);
  });

  it('空队列 consume 返回 null', () => {
    expect(useAttachmentInsertStore.getState().consume('s1')).toBeNull();
  });

  it('重复 enqueue 覆盖旧队列', () => {
    const other: AttachmentMeta = { ...META, name: 'b.png', kind: 'image' };
    useAttachmentInsertStore.getState().enqueue('s1', [META]);
    useAttachmentInsertStore.getState().enqueue('s2', [other]);
    expect(useAttachmentInsertStore.getState().consume('s1')).toBeNull();
    expect(useAttachmentInsertStore.getState().consume('s2')).toEqual([other]);
  });
});
