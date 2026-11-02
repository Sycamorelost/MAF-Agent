import { describe, it, expect } from 'vitest';
import { DEFAULT_BLOCKED_EXT } from './attachmentPolicy';
import { collectDroppedFiles, screenFiles, skipMessage } from './attachmentDrop';

/** 伪造文件条目(采集只读 name/size,不需要真实 File) */
function fileEntry(name: string, size = 1024): FileSystemEntry {
  const f = { name, size } as unknown as File;
  return {
    isFile: true,
    isDirectory: false,
    name,
    file: (ok: (f: File) => void) => ok(f),
  } as unknown as FileSystemEntry;
}

/** 伪造「读取失败」的文件条目(权限被拒 / 源文件已被删除) */
function brokenEntry(name: string): FileSystemEntry {
  return {
    isFile: true,
    isDirectory: false,
    name,
    file: (_ok: (f: File) => void, err: (e: unknown) => void) => err(new Error('denied')),
  } as unknown as FileSystemEntry;
}

/** 伪造目录条目;batch 用于模拟 readEntries 每次最多返回 100 项 */
function dirEntry(name: string, children: FileSystemEntry[], batch = 100): FileSystemEntry {
  let i = 0;
  return {
    isFile: false,
    isDirectory: true,
    name,
    createReader: () => ({
      readEntries: (ok: (entries: FileSystemEntry[]) => void) => {
        const chunk = children.slice(i, i + batch);
        i += batch;
        ok(chunk);
      },
    }),
  } as unknown as FileSystemEntry;
}

const OPTS = { folderLimit: 10, totalLimit: 10, blockedExt: DEFAULT_BLOCKED_EXT };
const names = (files: File[]) => files.map((f) => f.name).sort();

describe('拖入文件夹:极端场景', () => {
  it('多级文件夹递归展开,问题文件逐个拒绝、其余照常导入', async () => {
    const folder = dirEntry('assets', [
      dirEntry('docs', [fileEntry('a.pdf'), fileEntry('b.exe')]),
      fileEntry('c.md'),
      fileEntry('empty.txt', 0),
      fileEntry('big.zip', 21 * 1024 * 1024),
    ]);
    const r = await collectDroppedFiles([folder], [], OPTS);
    expect(names(r.files)).toEqual(['a.pdf', 'c.md']);
    expect(r.skipped).toEqual([
      { name: 'big.zip', reason: '超过 20MB 上限' },
      { name: 'b.exe', reason: '为格式黑名单文件' },
      { name: 'empty.txt', reason: '为空文件' },
    ]);
  });

  it('超过单文件夹阈值时截断并给出提示', async () => {
    const children = Array.from({ length: 15 }, (_, i) =>
      fileEntry(`f${String(i).padStart(2, '0')}.md`),
    );
    const r = await collectDroppedFiles([dirEntry('many', children)], [], {
      ...OPTS,
      folderLimit: 5,
    });
    expect(r.files).toHaveLength(5);
    expect(r.notices.join('|')).toContain('已按上限加入 5 个文件');
  });

  it('readEntries 分批(每次 100 项)也能读全并受阈值约束', async () => {
    const children = Array.from({ length: 250 }, (_, i) => fileEntry(`p${i}.txt`));
    const r = await collectDroppedFiles([dirEntry('huge', children, 100)], [], {
      ...OPTS,
      folderLimit: 8,
    });
    expect(r.files).toHaveLength(8);
    expect(r.files.every((f) => f.name.startsWith('p'))).toBe(true);
  });

  it('总附件上限为 0 时不导入并提示', async () => {
    const r = await collectDroppedFiles([dirEntry('x', [fileEntry('a.md')])], [], {
      ...OPTS,
      totalLimit: 0,
    });
    expect(r.files).toHaveLength(0);
    expect(r.notices.join('|')).toContain('附件数量上限');
  });

  it('多个文件夹共享总预算', async () => {
    const a = dirEntry('a', [fileEntry('a1.md'), fileEntry('a2.md')]);
    const b = dirEntry('b', [fileEntry('b1.md'), fileEntry('b2.md')]);
    const r = await collectDroppedFiles([a, b], [], { ...OPTS, totalLimit: 3 });
    expect(r.files).toHaveLength(3);
  });

  it('层级过深时停止并向用户说明', async () => {
    let node = dirEntry('d8', [fileEntry('deep.md')]);
    for (let i = 7; i >= 1; i--) node = dirEntry(`d${i}`, [node]);
    const r = await collectDroppedFiles([dirEntry('root', [node])], [], OPTS);
    expect(r.files).toHaveLength(0);
    expect(r.notices.join('|')).toContain('层级超过 8 层');
  });

  it('文件与文件夹混合拖入时按顺序展开', async () => {
    const r = await collectDroppedFiles(
      [fileEntry('top.md'), dirEntry('pkg', [fileEntry('inner.txt')])],
      [],
      OPTS,
    );
    expect(names(r.files)).toEqual(['inner.txt', 'top.md']);
  });

  it('拿不到目录条目时退回 files 列表并逐个过滤', async () => {
    const plain = [
      { name: 'ok.md', size: 1024 },
      { name: 'bad.exe', size: 1024 },
      { name: 'zero.txt', size: 0 },
    ] as unknown as File[];
    const r = await collectDroppedFiles([], plain, OPTS);
    expect(names(r.files)).toEqual(['ok.md']);
    expect(r.skipped.map((s) => s.name)).toEqual(['bad.exe', 'zero.txt']);
  });

  it('读取失败的文件单独记为「无法读取」,不影响同目录其他文件', async () => {
    const r = await collectDroppedFiles(
      [dirEntry('mixed', [fileEntry('a.md'), brokenEntry('locked.pdf'), fileEntry('c.txt')])],
      [],
      OPTS,
    );
    expect(names(r.files)).toEqual(['a.md', 'c.txt']);
    expect(r.skipped).toEqual([{ name: 'locked.pdf', reason: '无法读取' }]);
  });

  it('文件夹内全是黑名单文件时逐个列出,不误报「没有可添加的文件」', async () => {
    const r = await collectDroppedFiles(
      [dirEntry('danger', [fileEntry('a.EXE'), fileEntry('b.Bat'), fileEntry('c.ps1')])],
      [],
      OPTS,
    );
    expect(r.files).toHaveLength(0);
    expect(r.skipped.map((s) => s.name)).toEqual(['a.EXE', 'b.Bat', 'c.ps1']);
    expect(r.notices.join('|')).not.toContain('没有可添加的文件');
  });

  it('条目过多(全为黑名单文件)时只提示扫描上限,不出现「加入 0 个文件」', async () => {
    const children = Array.from({ length: 2001 }, (_, i) => fileEntry(`x${i}.exe`));
    const r = await collectDroppedFiles([dirEntry('huge', children, 100)], [], {
      ...OPTS,
      folderLimit: 10,
    });
    expect(r.files).toHaveLength(0);
    expect(r.notices.join('|')).toContain('条目过多');
    expect(r.notices.join('|')).not.toContain('加入 0 个文件');
  });
});

describe('选择/粘贴文件统一筛查', () => {
  it('只拒绝问题文件,保留其余', () => {
    const ok = { name: 'a.pdf', size: 100 } as unknown as File;
    const blocked = { name: 'b.bat', size: 10 } as unknown as File;
    const big = { name: 'c.iso', size: 21 * 1024 * 1024 } as unknown as File;
    const empty = { name: 'd.txt', size: 0 } as unknown as File;
    const r = screenFiles([ok, blocked, big, empty], DEFAULT_BLOCKED_EXT);
    expect(r.ok).toEqual([ok]);
    expect(r.skipped.map((s) => s.reason)).toEqual([
      '为格式黑名单文件',
      '超过 20MB 上限',
      '为空文件',
    ]);
  });
});

describe('拒绝提示文案', () => {
  it('逐个文件一行,一次性列出全部(不折叠)', () => {
    expect(skipMessage([{ name: 'a.exe', reason: '为格式黑名单文件' }])).toBe(
      '「a.exe」为格式黑名单文件，无法拖入对话',
    );
    const many = Array.from({ length: 10 }, (_, i) => ({
      name: `f${i}.exe`,
      reason: '为格式黑名单文件',
    }));
    const msg = skipMessage(many)!;
    // 全部 10 项都在(不再出现「另有 N 项」的截断行)
    expect(msg.split('\n')).toHaveLength(10);
    expect(msg).not.toContain('另有');
    expect(msg.split('\n')[9]).toContain('f9.exe');
  });

  it('文件夹截断提示排在文件拒绝提示之前', () => {
    const msg = skipMessage([{ name: 'a.exe', reason: '为格式黑名单文件' }], ['文件夹「x」已按上限加入 3 个文件,其余已跳过'])!;
    expect(msg.split('\n')[0]).toContain('文件夹「x」');
  });

  it('极端数量(2000 项)一次列全,不截断、不丢尾项', () => {
    const many = Array.from({ length: 2000 }, (_, i) => ({
      name: `f${i}.exe`,
      reason: '为格式黑名单文件',
    }));
    const msg = skipMessage(many)!;
    expect(msg.split('\n')).toHaveLength(2000);
    expect(msg).not.toContain('另有');
    expect(msg.endsWith('「f1999.exe」为格式黑名单文件，无法拖入对话')).toBe(true);
  });

  it('没有任何跳过项时返回 undefined(不弹空提示)', () => {
    expect(skipMessage([], [])).toBeUndefined();
  });
});
