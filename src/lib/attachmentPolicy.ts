//! 附件策略:单文件夹文件数阈值 + 扩展名黑名单(内置默认 ∪ 用户追加)。
//!
//! 与 Rust `attachment_guard::DEFAULT_BLOCKED_EXT` 保持一致;前端只做提示与预过滤,
//! 权威拦截在后端。采集/展开逻辑见 `attachmentDrop.ts`。

import * as ipc from './ipc';

/**
 * 内置默认黑名单(小写、不含点),与 Rust `attachment_guard::DEFAULT_BLOCKED_EXT` 保持一致。
 * 设置里的清单为空时回退到这里,避免「清空即放行」。
 */
export const DEFAULT_BLOCKED_EXT: string[] = [
  'exe', 'dll', 'scr', 'com', 'msi', 'msp', 'cpl', 'sys', 'drv',
  'bat', 'cmd', 'ps1', 'psm1', 'vbs', 'vbe', 'wsf', 'wsh',
  'jse', 'hta', 'reg', 'lnk', 'url', 'jar', 'apk', 'app',
];

/** 单文件夹可加入文件数的默认阈值 */
export const DEFAULT_FOLDER_LIMIT = 10;
/** 阈值上限(再大也受单条消息附件总数约束) */
export const MAX_FOLDER_LIMIT = 50;

/** 单会话附件数量配额:默认 200,上限 2000(与 Rust settings 保持一致) */
export const DEFAULT_SESSION_MAX_FILES = 200;
export const MAX_SESSION_MAX_FILES = 2000;
/** 单会话附件容量配额(MB):默认 1024,上限 10240 */
export const DEFAULT_SESSION_MAX_MB = 1024;
export const MAX_SESSION_MAX_MB = 10240;

export interface AttachmentPolicy {
  /** 每个文件夹最多加入的文件数 */
  folderLimit: number;
  /** 禁止加入的扩展名(小写、不含点) */
  blockedExt: string[];
}

/** 取扩展名(小写、不含点);`a.pdf.exe` → `exe`。 */
export function extOf(name: string): string {
  const i = name.lastIndexOf('.');
  return i >= 0 ? name.slice(i + 1).toLowerCase() : '';
}

/** 文件名是否命中黑名单。 */
export function isBlockedName(name: string, blockedExt: string[]): boolean {
  const ext = extOf(name);
  return !!ext && blockedExt.includes(ext);
}

/**
 * 把设置里的清单规整为扩展名数组:去点、转小写、按逗号/空白拆分并去重。
 * 内置默认清单始终生效(只允许追加,避免误清空后放行可执行文件)。
 */
export function normalizeExtList(raw: string[] | null | undefined): string[] {
  const set = new Set<string>(DEFAULT_BLOCKED_EXT);
  for (const item of raw ?? []) {
    for (const part of String(item).split(/[,，;；\s]+/)) {
      const e = part.trim().toLowerCase().replace(/^\.+/, '');
      if (e) set.add(e);
    }
  }
  return [...set];
}

/** 输入框文本 → 扩展名数组(空输入回退内置默认)。 */
export function parseExtInput(text: string): string[] {
  return normalizeExtList([text]);
}

/** 阈值钳制到 1..=MAX_FOLDER_LIMIT,非法值回退默认。 */
export function clampFolderLimit(n: unknown): number {
  const v = Number(n);
  if (!Number.isFinite(v) || v <= 0) return DEFAULT_FOLDER_LIMIT;
  return Math.min(MAX_FOLDER_LIMIT, Math.max(1, Math.floor(v)));
}

/** 会话数量配额钳制到 1..=MAX_SESSION_MAX_FILES,非法值回退默认。 */
export function clampSessionMaxFiles(n: unknown): number {
  const v = Number(n);
  if (!Number.isFinite(v) || v <= 0) return DEFAULT_SESSION_MAX_FILES;
  return Math.min(MAX_SESSION_MAX_FILES, Math.max(1, Math.floor(v)));
}

/** 会话容量配额(MB)钳制到 1..=MAX_SESSION_MAX_MB,非法值回退默认。 */
export function clampSessionMaxMb(n: unknown): number {
  const v = Number(n);
  if (!Number.isFinite(v) || v <= 0) return DEFAULT_SESSION_MAX_MB;
  return Math.min(MAX_SESSION_MAX_MB, Math.max(1, Math.floor(v)));
}

/** 读取当前附件策略(读取失败时回退内置默认,保证拦截不失效)。 */
export async function loadAttachmentPolicy(): Promise<AttachmentPolicy> {
  try {
    const s = await ipc.getSettings();
    return {
      folderLimit: clampFolderLimit(s.attachment_folder_limit),
      blockedExt: normalizeExtList(s.attachment_blocked_ext),
    };
  } catch {
    return { folderLimit: DEFAULT_FOLDER_LIMIT, blockedExt: DEFAULT_BLOCKED_EXT.slice() };
  }
}

/** 名称列表预览(最多 max 个,超出加「等」)。 */
export function previewNames(names: string[], max = 3): string {
  const shown = names.slice(0, max).join('、');
  return names.length > max ? `${shown} 等` : shown;
}
