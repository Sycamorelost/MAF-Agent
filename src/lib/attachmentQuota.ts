//! 附件配额用量计算(附件中心按会话展示;与 Rust `attachment_quota` 同口径)。
//!
//! 同一路径在一个会话内被多次复用只计一次,容量按元数据 size 求和。

import type { AttachmentEntry } from './ipc';

export interface QuotaUsage {
  /** 去重后的文件数 */
  files: number;
  /** 去重后的总字节数 */
  bytes: number;
}

export type QuotaTone = 'ok' | 'warn' | 'over';

const MB = 1024 * 1024;

/** 计算单个会话的用量(按路径去重);传入的条目应当是该会话的全部附件。 */
export function usageOf(entries: Array<Pick<AttachmentEntry, 'meta'>>): QuotaUsage {
  const seen = new Set<string>();
  let bytes = 0;
  for (const e of entries) {
    if (seen.has(e.meta.path)) continue;
    seen.add(e.meta.path);
    bytes += e.meta.size;
  }
  return { files: seen.size, bytes };
}

/** 按会话分组计算用量(过滤后的展示不影响配额口径)。 */
export function usageBySession(entries: AttachmentEntry[]): Map<string, QuotaUsage> {
  const paths = new Map<string, Set<string>>();
  const bytes = new Map<string, number>();
  for (const e of entries) {
    const sid = e.session_id;
    let seen = paths.get(sid);
    if (!seen) {
      seen = new Set();
      paths.set(sid, seen);
    }
    if (seen.has(e.meta.path)) continue;
    seen.add(e.meta.path);
    bytes.set(sid, (bytes.get(sid) ?? 0) + e.meta.size);
  }
  const out = new Map<string, QuotaUsage>();
  for (const [sid, seen] of paths) {
    out.set(sid, { files: seen.size, bytes: bytes.get(sid) ?? 0 });
  }
  return out;
}

/** 配额状态:≥80% 警告,超限标红。 */
export function quotaTone(usage: QuotaUsage, maxFiles: number, maxMb: number): QuotaTone {
  const maxBytes = Math.max(1, maxMb) * MB;
  if (usage.files > Math.max(1, maxFiles) || usage.bytes > maxBytes) return 'over';
  const ratio = Math.max(usage.files / Math.max(1, maxFiles), usage.bytes / maxBytes);
  return ratio >= 0.8 ? 'warn' : 'ok';
}

/** 用量文案:`12/200 个 · 3.4/1024 MB`。 */
export function quotaLabel(usage: QuotaUsage, maxFiles: number, maxMb: number): string {
  const usedMb = (usage.bytes / MB).toFixed(1);
  return `${usage.files}/${maxFiles} 个 · ${usedMb}/${maxMb} MB`;
}
