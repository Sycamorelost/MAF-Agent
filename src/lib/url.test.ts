import { describe, it, expect } from 'vitest';
import { sanitizeHttpUrl } from './url';

describe('sanitizeHttpUrl', () => {
  it('放行 http 链接', () => {
    expect(sanitizeHttpUrl('http://example.com/a?b=1')).toBe('http://example.com/a?b=1');
  });

  it('放行 https 链接', () => {
    expect(sanitizeHttpUrl('https://example.com/x#y')).toBe('https://example.com/x#y');
  });

  it('拒绝 javascript: 协议(XSS 防护)', () => {
    expect(sanitizeHttpUrl('javascript:alert(1)')).toBeNull();
  });

  it('拒绝 data: 协议', () => {
    expect(sanitizeHttpUrl('data:text/html,<script>alert(1)</script>')).toBeNull();
  });

  it('拒绝 ftp: 等其他协议', () => {
    expect(sanitizeHttpUrl('ftp://example.com/f')).toBeNull();
  });

  it('非法 URL 字符串返回 null 而不抛错', () => {
    expect(sanitizeHttpUrl('not a url')).toBeNull();
    expect(sanitizeHttpUrl('')).toBeNull();
  });

  it('协议相对地址(无 base)解析失败返回 null', () => {
    expect(sanitizeHttpUrl('//evil.com/x')).toBeNull();
  });
});
