import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

/**
 * 源文件体积守卫:拒绝超大文件回潮。
 *
 * 规则:
 * - 新文件(不在基线表)单文件不超过 400 行 / 24KB;
 * - 存量文件登记在 LEGACY 基线中,只允许缩行,不允许上调;降到 400 行以内后
 *   必须从基线表删除(测试会校验基线值必须 >400,防止“基线永久豁免”);
 * - 生成物(src-tauri/gen)与构建产物不参与检查。
 */
const ROOT = resolve(process.cwd());
const MAX_LINES = 400;
const MAX_BYTES = 24 * 1024;
const LEGACY_MAX_BYTES = 40 * 1024;

/** 存量基线:路径 → 当前行数(只降不升)。路径统一用 `/`。 */
const LEGACY: Record<string, number> = {
  'src/styles/chat/composer.css': 691,
  'src-tauri/src/commands/provider/context_budget.rs': 673,
  'src-tauri/src/provider/keychain.rs': 655,
  'src/styles/settings/tokens.css': 583,
  'src-tauri/src/commands/attachment/center.rs': 579,
  'src-tauri/src/notes_presets.rs': 560,
  'src/styles/chat/enhancements.css': 557,
  'src-tauri/src/db/schema/mod.rs': 527,
  'src-tauri/src/fs/docgen/inline.rs': 522,
  'src-tauri/src/commands/provider/keys.rs': 500,
  'src-tauri/src/provider/model_cards/vendor.rs': 494,
  'src/pages/settings/ProviderSettingsCard.tsx': 479,
  'src-tauri/src/attachment.rs': 478,
  'src/lib/ipc/provider.ts': 477,
  'src/components/agent/AgentCard.tsx': 472,
  'src-tauri/src/provider/rotation.rs': 471,
  'src-tauri/src/provider/openai/tests.rs': 467,
  'src-tauri/src/db/schema/tests.rs': 463,
  'src-tauri/src/fs/text/tests.rs': 460,
  'src-tauri/src/commands/notes.rs': 459,
  'src-tauri/src/db/messages/crud.rs': 454,
  'src-tauri/src/doc/search.rs': 449,
  'src-tauri/src/websearch/engine/tests.rs': 446,
  'src-tauri/src/websearch/page/fetch.rs': 445,
  'src/styles/_tokens/components.css': 444,
  'src/stores/messages.ts': 440,
  'src-tauri/src/mcp/safety.rs': 435,
  'src-tauri/src/gui/uia.rs': 423,
  'src-tauri/src/db/sessions.rs': 412,
  'src/lib/toolTrace.ts': 407,
  'src/pages/NotesPage.tsx': 404,
  'src-tauri/src/commands/memory/crud.rs': 403,
  'src-tauri/src/mcp/dispatcher.rs': 402,
};

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
  it(`新文件不超过 ${MAX_LINES} 行 / ${MAX_BYTES / 1024}KB,存量只降不升`, () => {
    const offenders = SCAN.flatMap(({ dir, exts }) => walk(dir, exts)).flatMap((full) => {
      const text = readFileSync(full, 'utf8');
      const lines = countLines(text);
      const bytes = Buffer.byteLength(text, 'utf8');
      const rel = relative(ROOT, full).replace(/\\/g, '/');
      const limit = LEGACY[rel];
      const maxLines = limit ?? MAX_LINES;
      const maxBytes = limit ? LEGACY_MAX_BYTES : MAX_BYTES;
      const tooLong = lines > maxLines;
      const tooBig = bytes > maxBytes;
      if (!tooLong && !tooBig) return [];
      const where = `${rel} (${lines} 行 / ${(bytes / 1024).toFixed(1)}KB)`;
      return [
        where +
          (tooLong ? ` 超出 ${lines - maxLines} 行,请拆分到 ≤${MAX_LINES} 行` : '') +
          (tooBig ? ` 超出 ${((bytes - maxBytes) / 1024).toFixed(1)}KB,请抽出子模块` : ''),
      ];
    });
    expect(offenders.sort()).toEqual([]);
  });

  it('基线条目必须仍然超过 400 行(缩到 400 以内请删除条目)', () => {
    const stale = Object.entries(LEGACY)
      .filter(([, lines]) => lines <= MAX_LINES)
      .map(([path, lines]) => `${path} 已降到 ${lines} 行,请从 LEGACY 删除`);
    expect(stale.sort()).toEqual([]);
  });
});
