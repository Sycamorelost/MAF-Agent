/** 外部 URL 安全校验:仅放行 http/https,其他协议(javascript: 等)返回 null。
 *  用于渲染不可信来源(搜索引擎结果等)的链接前做白名单过滤。
 *  依据:OWASP XSS Prevention Cheat Sheet —— 不可信 URL 必须校验协议。
 */
export function sanitizeHttpUrl(url: string): string | null {
  try {
    const u = new URL(url);
    if (u.protocol === 'http:' || u.protocol === 'https:') {
      return url;
    }
    return null;
  } catch {
    return null;
  }
}
