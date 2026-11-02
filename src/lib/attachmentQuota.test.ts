import { describe, it, expect } from 'vitest';
import { quotaLabel, quotaTone, usageBySession, usageOf } from './attachmentQuota';
import type { AttachmentEntry } from './ipc';

const entry = (sessionId: string, path: string, size: number): AttachmentEntry => ({
  message_id: path.length,
  session_id: sessionId,
  session_title: sessionId,
  branch_id: 'main',
  created: '2026-11-01T00:00:00Z',
  meta: { name: path.split('\\').pop() ?? path, path, kind: 'text', size, extracted_text: null },
});

describe('附件配额用量', () => {
  it('同一路径只计一次', () => {
    const u = usageOf([entry('s1', 'a.txt', 10), entry('s1', 'a.txt', 10), entry('s1', 'b.txt', 20)]);
    expect(u.files).toBe(2);
    expect(u.bytes).toBe(30);
  });

  it('按会话分组互不串味', () => {
    const all = [
      entry('s1', 'x\\a', 5),
      entry('s2', 'y\\a', 7),
      entry('s2', 'y\\b', 3),
      entry('s2', 'y\\b', 3),
    ];
    const map = usageBySession(all);
    expect(map.get('s1')).toEqual({ files: 1, bytes: 5 });
    expect(map.get('s2')).toEqual({ files: 2, bytes: 10 });
  });

  it('80% 警告与超限标红', () => {
    expect(quotaTone({ files: 160, bytes: 0 }, 200, 1024)).toBe('warn');
    expect(quotaTone({ files: 10, bytes: 900 * 1024 * 1024 }, 200, 1024)).toBe('warn');
    expect(quotaTone({ files: 100, bytes: 0 }, 200, 1024)).toBe('ok');
    // 数量 50% 且容量 0% 时为正常
    expect(quotaTone({ files: 10, bytes: 0 }, 20, 1024)).toBe('ok');
    expect(quotaTone({ files: 201, bytes: 0 }, 200, 1024)).toBe('over');
    expect(quotaTone({ files: 0, bytes: 1025 * 1024 * 1024 }, 200, 1024)).toBe('over');
  });

  it('用量文案带 MB 与总数', () => {
    expect(quotaLabel({ files: 12, bytes: 3.4 * 1024 * 1024 }, 200, 1024)).toBe('12/200 个 · 3.4/1024 MB');
    expect(quotaLabel({ files: 0, bytes: 0 }, 200, 1024)).toBe('0/200 个 · 0.0/1024 MB');
  });

  it('空数据与零配额不产生除零异常', () => {
    expect(usageOf([])).toEqual({ files: 0, bytes: 0 });
    expect(usageBySession([]).size).toBe(0);
    // 配额被钳制为 0 的异常配置:按 1 计算,不出现 NaN / Infinity
    expect(quotaTone({ files: 0, bytes: 0 }, 0, 0)).toBe('ok');
    expect(quotaTone({ files: 5, bytes: 0 }, 0, 0)).toBe('over');
    expect(quotaLabel({ files: 0, bytes: 0 }, 0, 0)).toBe('0/0 个 · 0.0/0 MB');
  });

  it('海量重复路径仍按去重统计', () => {
    const many = Array.from({ length: 500 }, () => entry('s1', 'big\\a.pdf', 2048));
    expect(usageOf(many)).toEqual({ files: 1, bytes: 2048 });
    expect(usageBySession(many).get('s1')).toEqual({ files: 1, bytes: 2048 });
  });
});
