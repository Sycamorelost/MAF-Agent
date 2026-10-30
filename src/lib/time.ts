/**
 * 后端时间统一解析。
 * SQLite 的 `datetime('now')` 产出 `YYYY-MM-DD HH:MM:SS`(空格分隔,不是 ISO),
 * 各 WebView 对空格格式的解析实现不一致,直接 `new Date(str)` 可能得到 Invalid Date,
 * 导致会话分组、相对时间错误。所有展示层统一走这里。
 */

/** 解析后端时间戳为毫秒;非法值返回 0。 */
export function parseDbTime(value: string | null | undefined): number {
  if (!value) return 0;
  const t = new Date(value.replace(' ', 'T')).getTime();
  return Number.isNaN(t) ? 0 : t;
}

/** 相对时间:刚刚 / N 分钟前 / N 小时前 / N 天前;超过 30 天显示日期。 */
export function fmtRelative(value: string, now = Date.now()): string {
  const t = parseDbTime(value);
  if (!t) return '未知';
  const mins = Math.floor((now - t) / 60000);
  if (mins < 1) return '刚刚';
  if (mins < 60) return `${mins} 分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  return new Date(t).toLocaleDateString('zh-CN');
}
