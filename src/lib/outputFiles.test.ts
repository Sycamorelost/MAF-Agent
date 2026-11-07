import { describe, expect, it } from 'vitest';
import {
  extractOutputFiles,
  normalizeOutputPath,
  outputBaseName,
} from './outputFiles';

describe('normalizeOutputPath', () => {
  it('保留 Windows 绝对路径', () => {
    expect(normalizeOutputPath('C:\\Users\\me\\out\\report.md')).toBe(
      'C:\\Users\\me\\out\\report.md',
    );
  });

  it('file:/// 转为盘符路径', () => {
    expect(normalizeOutputPath('file:///C:/tmp/a.png')).toBe('C:/tmp/a.png');
  });

  it('去掉末尾标点', () => {
    expect(normalizeOutputPath('/tmp/a.png)')).toBe('/tmp/a.png');
  });

  it('过滤网络与内联地址', () => {
    expect(normalizeOutputPath('https://a.com/x.png')).toBeNull();
    expect(normalizeOutputPath('data:image/png;base64,AAA')).toBeNull();
    expect(normalizeOutputPath('mailto:a@b.com')).toBeNull();
  });

  it('非法百分号编码不抛异常', () => {
    expect(normalizeOutputPath('file:///C:/a%ZZ/b.png')).toBe('C:/a%ZZ/b.png');
  });

  it('拒绝无目录的纯文件名与非文件后缀', () => {
    expect(normalizeOutputPath('config.json')).toBeNull();
    expect(normalizeOutputPath('C:\\tmp\\a.unknownext')).toBeNull();
  });
});

describe('outputBaseName', () => {
  it('兼容两种分隔符', () => {
    expect(outputBaseName('C:\\a\\b\\c.pdf')).toBe('c.pdf');
    expect(outputBaseName('/a/b/c.pdf')).toBe('c.pdf');
  });
});

describe('extractOutputFiles', () => {
  it('识别 Markdown 图片与链接', () => {
    const text = '见 ![图](C:\\out\\a.png) 和 [报告](/home/me/out/r.md)';
    const files = extractOutputFiles(text);
    expect(files.map((f) => f.name)).toEqual(['a.png', 'r.md']);
    expect(files[0].isImage).toBe(true);
    expect(files[1].isImage).toBe(false);
  });

  it('识别 file:// 与裸绝对路径', () => {
    const text = '产物:file:///D:/data/x.zip 以及 D:\\logs\\y.txt';
    const files = extractOutputFiles(text);
    expect(files.map((f) => f.path)).toEqual(['D:/data/x.zip', 'D:\\logs\\y.txt']);
  });

  it('忽略网络链接与普通文件名', () => {
    const text = '参考 https://example.com/a.png 和 package.json';
    expect(extractOutputFiles(text)).toHaveLength(0);
  });

  it('同一文件只出现一次,大小写与分隔符归一', () => {
    const text = 'C:\\Out\\A.PNG 与 C:/out/a.png';
    expect(extractOutputFiles(text)).toHaveLength(1);
  });

  it('最多返回 12 个,且空内容安全', () => {
    const text = Array.from({ length: 20 }, (_, i) => `C:\\out\\f${i}.txt`).join(' ');
    expect(extractOutputFiles(text)).toHaveLength(12);
    expect(extractOutputFiles('')).toHaveLength(0);
  });
});
