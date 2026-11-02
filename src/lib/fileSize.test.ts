import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

/**
 * 源文件体积守卫:拒绝超大文件回潮。
 * 约定:前端源码/样式与 Rust 源码单个文件不超过 700 行(或 40KB)。
 * 生成物(src-tauri/gen)与构建产物不参与检查。
 */
const ROOT = resolve(process.cwd());
const MAX_LINES = 700;
const MAX_BYTES = 40 * 1024;
const SCAN: { dir: string; exts: string[] }[] = [
  { dir: join(ROOT, 'src'), exts: ['.ts', '.tsx', '.css'] },
  { dir: join(ROOT, 'src-tauri', 'src'), exts: ['.rs'] },
];

function walk(dir: string, exts: string[], out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      walk(full, exts, out);
    } else if (exts.some((e) => name.endsWith(e))) {
      out.push(full);
    }
  }
  return out;
}

function countLines(text: string): number {
  const lines = text.split(/\r?\n/);
  if (lines[lines.length - 1] === '') lines.pop();
  return lines.length;
}

describe('源文件体积守卫', () => {
  it(`单个源文件不超过 ${MAX_LINES} 行 / ${MAX_BYTES / 1024}KB`, () => {
    const offenders = SCAN.flatMap(({ dir, exts }) => walk(dir, exts)).flatMap((full) => {
      const text = readFileSync(full, 'utf8');
      const lines = countLines(text);
      const bytes = Buffer.byteLength(text, 'utf8');
      const tooLong = lines > MAX_LINES;
      const tooBig = bytes > MAX_BYTES;
      if (!tooLong && !tooBig) return [];
      return [
        `${relative(ROOT, full)} (${lines} 行 / ${(bytes / 1024).toFixed(1)}KB)` +
          (tooLong ? ` 超出 ${lines - MAX_LINES} 行` : '') +
          (tooBig ? ` 超出 ${((bytes - MAX_BYTES) / 1024).toFixed(1)}KB` : ''),
      ];
    });
    expect(offenders.sort()).toEqual([]);
  });
});
