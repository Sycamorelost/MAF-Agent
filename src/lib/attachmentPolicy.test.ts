import { describe, it, expect } from 'vitest';
import {
  DEFAULT_BLOCKED_EXT,
  DEFAULT_FOLDER_LIMIT,
  DEFAULT_SESSION_MAX_FILES,
  DEFAULT_SESSION_MAX_MB,
  MAX_FOLDER_LIMIT,
  MAX_SESSION_MAX_FILES,
  MAX_SESSION_MAX_MB,
  clampFolderLimit,
  clampSessionMaxFiles,
  clampSessionMaxMb,
  extOf,
  isBlockedName,
  normalizeExtList,
  parseExtInput,
  previewNames,
} from './attachmentPolicy';

describe('附件策略', () => {
  it('扩展名取最后一段并转小写', () => {
    expect(extOf('a.pdf.exe')).toBe('exe');
    expect(extOf('REPORT.PDF')).toBe('pdf');
    expect(extOf('noext')).toBe('');
  });

  it('黑名单命中可执行与脚本类型', () => {
    expect(isBlockedName('setup.exe', DEFAULT_BLOCKED_EXT)).toBe(true);
    expect(isBlockedName('run.BAT', DEFAULT_BLOCKED_EXT)).toBe(true);
    expect(isBlockedName('report.pdf', DEFAULT_BLOCKED_EXT)).toBe(false);
    // 不误伤文本类代码附件
    expect(isBlockedName('main.py', DEFAULT_BLOCKED_EXT)).toBe(false);
  });

  it('内置默认清单不可被清空(只能追加)', () => {
    const list = normalizeExtList([]);
    expect(list).toContain('exe');
    expect(new Set(list).size).toBe(DEFAULT_BLOCKED_EXT.length);
    const withExtra = normalizeExtList(['.iso, rar']);
    expect(withExtra).toContain('iso');
    expect(withExtra).toContain('rar');
    expect(withExtra).toContain('exe');
  });

  it('输入框文本解析为空时仍保留内置默认', () => {
    expect(parseExtInput('   ')).toContain('exe');
    expect(parseExtInput('7z  zip')).toEqual(expect.arrayContaining(['7z', 'zip']));
  });

  it('阈值钳制到 1..=上限,非法值回退默认', () => {
    expect(clampFolderLimit(20)).toBe(20);
    expect(clampFolderLimit(0)).toBe(DEFAULT_FOLDER_LIMIT);
    expect(clampFolderLimit('abc')).toBe(DEFAULT_FOLDER_LIMIT);
    expect(clampFolderLimit(999)).toBe(MAX_FOLDER_LIMIT);
    expect(clampFolderLimit(2.7)).toBe(2);
  });

  it('会话配额钳制到 1..=上限,非法值回退默认', () => {
    expect(clampSessionMaxFiles(50)).toBe(50);
    expect(clampSessionMaxFiles(0)).toBe(DEFAULT_SESSION_MAX_FILES);
    expect(clampSessionMaxFiles(-3)).toBe(DEFAULT_SESSION_MAX_FILES);
    expect(clampSessionMaxFiles('abc')).toBe(DEFAULT_SESSION_MAX_FILES);
    expect(clampSessionMaxFiles(null)).toBe(DEFAULT_SESSION_MAX_FILES);
    expect(clampSessionMaxFiles(9999)).toBe(MAX_SESSION_MAX_FILES);
    expect(clampSessionMaxFiles(12.9)).toBe(12);

    expect(clampSessionMaxMb(512)).toBe(512);
    expect(clampSessionMaxMb(0)).toBe(DEFAULT_SESSION_MAX_MB);
    expect(clampSessionMaxMb('x')).toBe(DEFAULT_SESSION_MAX_MB);
    expect(clampSessionMaxMb(99999)).toBe(MAX_SESSION_MAX_MB);
    expect(clampSessionMaxMb(1.5)).toBe(1);
  });

  it('名称预览最多列 3 个', () => {
    expect(previewNames(['a.exe'])).toBe('a.exe');
    expect(previewNames(['a.exe', 'b.dll', 'c.scr', 'd.bat'])).toBe('a.exe、b.dll、c.scr 等');
  });
});
