//! 拖入/粘贴文件的统一采集:文件夹递归展开、单文件夹数量阈值、扩展名黑名单。
//!
//! 这里只是前端的即时提示与预过滤;真正可信的拦截在 Rust 侧
//! (`attachment_guard::check_attachment_allowed`,由 save_attachment / extract_text_preview 调用)。
//! 阈值/黑名单的读取与规整见 `attachmentPolicy.ts`。

import * as ipc from './ipc';
import { isBlockedName } from './attachmentPolicy';

const MAX_FILE_BYTES = ipc.MAX_ATTACHMENT_BYTES;

/** 递归深度上限(防御异常深的目录树) */
const MAX_DEPTH = 8;
/** 单个文件夹遍历条目上限(防御海量条目拖住 UI) */
const MAX_VISIT = 2000;

/**
 * 在 drop 事件的同步阶段取出目录条目。
 * DataTransfer 在 await 之后可能被清空,所以必须先同步取出再异步展开。
 */
export function entriesOf(dt: DataTransfer): FileSystemEntry[] {
  const items = dt.items;
  if (!items || items.length === 0) return [];
  const out: FileSystemEntry[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item.kind !== 'file') continue;
    const entry = item.webkitGetAsEntry?.() ?? null;
    if (entry) out.push(entry);
  }
  return out;
}

export interface CollectOptions {
  /** 每个文件夹最多加入的文件数 */
  folderLimit: number;
  /** 本次还能加入的总数(单条消息附件上限减去已有数) */
  totalLimit: number;
  blockedExt: string[];
}

/** 被拒绝加入的文件及原因(用于逐个提示,而不是整批拒绝) */
export interface SkippedFile {
  name: string;
  /** 中文原因,可直接拼进「「x」{reason}，无法拖入对话」 */
  reason: string;
}

export interface CollectResult {
  files: File[];
  /** 被拒绝加入的单个文件 */
  skipped: SkippedFile[];
  /** 文件夹层面的提示(截断 / 空目录 / 过深) */
  notices: string[];
}

/** 单个文件能否加入;不能则给出类别与中文原因。 */
function skipReason(
  f: { name: string; size: number },
  blockedExt: string[],
): { kind: 'blocked' | 'empty' | 'oversize'; text: string } | null {
  if (isBlockedName(f.name, blockedExt)) return { kind: 'blocked', text: '为格式黑名单文件' };
  if (f.size <= 0) return { kind: 'empty', text: '为空文件' };
  if (f.size > MAX_FILE_BYTES) {
    return { kind: 'oversize', text: `超过 ${Math.round(MAX_FILE_BYTES / 1024 / 1024)}MB 上限` };
  }
  return null;
}

/**
 * 逐个筛掉不允许加入的文件(选择器 / 粘贴 / 拖入共用)。
 * 只拒绝问题文件本身,其余照常导入。
 */
export function screenFiles(
  files: File[],
  blockedExt: string[],
): { ok: File[]; skipped: SkippedFile[] } {
  const ok: File[] = [];
  const skipped: SkippedFile[] = [];
  for (const f of files) {
    const reason = skipReason(f, blockedExt);
    if (reason) skipped.push({ name: f.name, reason: reason.text });
    else ok.push(f);
  }
  return { ok, skipped };
}

/** 组装逐个文件的拒绝提示(一次性列出全部,由提示面板滚动查看);无拒绝项返回 undefined。 */
export function skipMessage(
  skipped: SkippedFile[],
  notes: string[] = [],
): string | undefined {
  const lines = [
    ...notes,
    ...skipped.map((s) => `「${s.name}」${s.reason}，无法拖入对话`),
  ];
  return lines.length > 0 ? lines.join('\n') : undefined;
}

/** 单个文件夹的遍历统计 */
interface FolderStat {
  name: string;
  added: number;
  blocked: number;
  empty: number;
  oversize: number;
  unreadable: number;
  visited: number;
  truncated: boolean;
  deep: boolean;
}

function fileOfEntry(e: FileSystemFileEntry): Promise<File | null> {
  return new Promise((res) => e.file((f) => res(f), () => res(null)));
}

/** 读取目录下全部条目(每次最多 100 项,必须循环读到空数组)。 */
async function readAllEntries(dir: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = dir.createReader();
  const all: FileSystemEntry[] = [];
  try {
    for (let i = 0; i < 40; i++) {
      const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
      if (!batch || batch.length === 0) break;
      all.push(...batch);
    }
  } catch {
    /* 中途读取失败时返回已拿到的部分 */
  }
  return all.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));
}

/** 递归收集一个文件夹内的文件(受 budget 与 MAX_VISIT 约束)。 */
async function walkDir(
  dir: FileSystemDirectoryEntry,
  budget: number,
  blockedExt: string[],
  out: File[],
  stat: FolderStat,
  skipped: SkippedFile[],
  depth: number,
): Promise<void> {
  if (stat.added >= budget) {
    stat.truncated = true;
    return;
  }
  if (depth > MAX_DEPTH) {
    stat.deep = true;
    return;
  }
  const entries = await readAllEntries(dir);
  for (const e of entries) {
    if (stat.added >= budget || stat.visited >= MAX_VISIT) {
      stat.truncated = true;
      return;
    }
    if (e.isFile) {
      stat.visited += 1;
      const f = await fileOfEntry(e as FileSystemFileEntry);
      if (!f) {
        stat.unreadable += 1;
        skipped.push({ name: e.name, reason: '无法读取' });
        continue;
      }
      const reason = skipReason(f, blockedExt);
      if (reason) {
        stat[reason.kind] += 1;
        skipped.push({ name: f.name, reason: reason.text });
        continue;
      }
      out.push(f);
      stat.added += 1;
    } else if (e.isDirectory) {
      stat.visited += 1;
      await walkDir(e as FileSystemDirectoryEntry, budget, blockedExt, out, stat, skipped, depth + 1);
    }
  }
}

/**
 * 把拖入内容展开为文件列表:
 * - 文件夹递归收集,每个文件夹最多 `folderLimit` 个(再受 `totalLimit` 约束);
 * - 不允许加入的文件逐个记录到 `skipped`(黑名单 / 空文件 / 超 20MB),其余照常导入;
 * - 拿不到目录信息(旧内核、剪贴板)时退回 `plainFiles`。
 */
export async function collectDroppedFiles(
  entries: FileSystemEntry[],
  plainFiles: File[],
  opts: CollectOptions,
): Promise<CollectResult> {
  const files: File[] = [];
  const notices: string[] = [];
  const skipped: SkippedFile[] = [];

  if (entries.length === 0) {
    const screened = screenFiles(plainFiles, opts.blockedExt);
    return { files: screened.ok, skipped: screened.skipped, notices };
  }

  let budget = Math.max(0, opts.totalLimit);
  let totalTruncated = false;

  for (const entry of entries) {
    if (budget <= 0) {
      totalTruncated = true;
      break;
    }
    if (entry.isFile) {
      const f = await fileOfEntry(entry as FileSystemFileEntry);
      if (!f) {
        skipped.push({ name: entry.name, reason: '无法读取' });
        continue;
      }
      const reason = skipReason(f, opts.blockedExt);
      if (reason) {
        skipped.push({ name: f.name, reason: reason.text });
        continue;
      }
      files.push(f);
      budget -= 1;
      continue;
    }
    if (!entry.isDirectory) continue;

    const stat: FolderStat = {
      name: entry.name,
      added: 0,
      blocked: 0,
      empty: 0,
      oversize: 0,
      unreadable: 0,
      visited: 0,
      truncated: false,
      deep: false,
    };
    const start = files.length;
    await walkDir(
      entry as FileSystemDirectoryEntry,
      Math.min(opts.folderLimit, budget),
      opts.blockedExt,
      files,
      stat,
      skipped,
      1,
    );
    const folderFiles = files.slice(start);
    const nothing = stat.blocked + stat.empty + stat.oversize + stat.unreadable;
    if (folderFiles.length === 0 && nothing === 0 && !stat.deep) {
      notices.push(`文件夹「${stat.name}」内没有可添加的文件`);
    }
    // 仅在确实加入过文件时提示「按上限」,避免条目过多(加入 0 个)时出现 0 个文件的误导文案
    if (stat.truncated && folderFiles.length > 0) {
      notices.push(`文件夹「${stat.name}」已按上限加入 ${folderFiles.length} 个文件,其余已跳过`);
    }
    if (stat.deep) {
      notices.push(`文件夹「${stat.name}」层级超过 ${MAX_DEPTH} 层,更深的目录已跳过`);
    }
    budget -= stat.added;
    if (stat.visited >= MAX_VISIT) {
      notices.push(`文件夹「${stat.name}」条目过多,仅扫描了前 ${MAX_VISIT} 项`);
    }
  }

  if (totalTruncated) {
    notices.push('受单条消息附件数量上限约束,其余文件已跳过');
  }
  return { files, skipped, notices };
}
