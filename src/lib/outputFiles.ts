/** 会话产物路径识别(纯函数,便于单测)。
 *
 * AI 回复里出现的本地文件可能以三种形式出现:
 * 1. Markdown 链接 / 图片:`[报告](out/report.md)`、`![图](C:\tmp\a.png)`
 * 2. file:// URL
 * 3. 裸绝对路径:`C:\Users\me\a.pdf`、`/tmp/a.png`
 *
 * 这里只做「文本 → 候选路径」的提取,是否真的存在由后端 `fs_reveal` /
 * `fs_read_data_url` 决定,前端不猜测文件系统。
 */

const FILE_EXT =
  /\.(png|jpe?g|gif|webp|bmp|svg|pdf|docx?|xlsx?|pptx?|csv|tsv|txt|md|json|ya?ml|zip|7z|tar|gz|mp4|webm|mp3|wav|html?|rs|ts|tsx|js|jsx|py)$/i;

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;

export interface OutputFile {
  /** 原始路径(交给后端按工作区解析) */
  path: string;
  /** 展示用文件名 */
  name: string;
  isImage: boolean;
}

/** 去掉尾随标点/包裹符,统一 file:// 前缀,并过滤网络地址。 */
export function normalizeOutputPath(raw: string): string | null {
  let p = raw.trim().replace(/^[<`"']+|[>`"']+$/g, '');
  if (p.startsWith('file://')) {
    const raw = p.slice('file://'.length);
    try {
      p = decodeURIComponent(raw);
    } catch {
      p = raw; // 非法百分号编码时退回原文,不让渲染崩掉
    }
    // file:///C:/x → C:/x
    p = p.replace(/^\/([A-Za-z]:)/, '$1');
  }
  if (/^(https?:|data:|blob:|mailto:|javascript:)/i.test(p)) return null;
  p = p.replace(/[.,;:)\]}>'"]+$/, '');
  if (!FILE_EXT.test(p)) return null;
  // 纯文件名(无目录分隔)不当作产物,避免把正文里的 `config.json` 全部卡片化
  if (!/[\\/]/.test(p)) return null;
  return p;
}

/** 取路径最后一段作为展示名。 */
export function outputBaseName(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || p;
}

/** 从消息正文提取本地文件引用(去重、保持出现顺序、最多 12 个)。 */
export function extractOutputFiles(content: string): OutputFile[] {
  if (!content || content.length < 4) return [];
  const seen = new Set<string>();
  const out: OutputFile[] = [];
  const push = (raw: string) => {
    const p = normalizeOutputPath(raw);
    if (!p) return;
    const key = p.replace(/\\/g, '/').toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    out.push({ path: p, name: outputBaseName(p), isImage: IMAGE_EXT.test(p) });
  };

  // 1) Markdown 链接与图片
  for (const m of content.matchAll(/!?\[[^\]]*\]\(([^)\n]+)\)/g)) push(m[1]);
  // 2) file:// URL
  for (const m of content.matchAll(/file:\/\/[^\s<>"'`)]+/g)) push(m[0]);
  // 3) 裸绝对路径:Windows 盘符 / POSIX
  //    前置负向断言排除 `file:` / `https:` 里被误认成盘符的 `e:`、`s:`
  for (const m of content.matchAll(/(?<![A-Za-z0-9_])[A-Za-z]:[\\/][^\s<>"'`|?*]+/g)) push(m[0]);
  for (const m of content.matchAll(/(?:^|[\s(])(\/(?:[\w.-]+\/)+[\w.-]+)/g)) push(m[1]);

  // 4) 根目录发现 + 纯文件名补全:
  //    AI 经常会说 "已在 D:\NUDT\草稿 创建了 foo.txt、bar.txt",
  //    这里先找到目录,再把孤立的 .txt/.md 文件补成完整路径。
  const rootDir = discoverRootDir(content);
  if (rootDir) {
    for (const m of content.matchAll(/(?<![\\/])([\u4e00-\u9fa5A-Za-z0-9_.-]+\.(?:txt|md|json|csv|doc|docx|xlsx|pptx|pdf|html|rs|ts|js|py))/gi)) {
      const name = m[1];
      // 过滤已经被完整路径提过的(用 seen 检查)
      const full = rootDir.replace(/[\\/]+$/, '') + '\\' + name;
      const key = full.replace(/\\/g, '/').toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ path: full, name, isImage: IMAGE_EXT.test(name) });
    }
  }

  return out.slice(0, 12);
}

/** 从 AI 回复里提取工作目录:
 *  - Windows:`D:\xxx\yyy` / `D:/xxx/yyy` 或"目录:xxx"
 *  - POSIX:`/home/user/xxx`、`~/xxx`
 * 取第一个出现的、且不在代码块内的目录。 */
function discoverRootDir(content: string): string | null {
  // 跳过代码块(```...```) 里的内容,避免把 Linux 代码里的 /tmp/xxx 当成目录
  const outside = content.replace(/```[\s\S]*?```/g, '');
  // Windows 完整路径(负向断言排除 https:// 里被误认成盘符的 s:/)
  const winMatch = outside.match(/(?<![A-Za-z0-9_])[A-Za-z]:[\\/][^\s<>"'`|?*]{3,}?[\\/][^\s<>"'`|?*]+/);
  if (winMatch) return winMatch[0];
  // POSIX /home 或 /tmp 开头
  const posixMatch = outside.match(/(?:^|[\s(])(\/(?:home|tmp|var|usr|opt|data|workspace|Users|mnt)[^\s<>"'`|?*]+)/);
  if (posixMatch) return posixMatch[1];
  return null;
}
