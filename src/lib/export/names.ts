//! 导出文件名工具(P0 自 pages/research/helpers.ts 迁出,供会话/学习/研究共用)。

/** 导出文件名:仅保留安全字符(Windows 保留字符与控制符剔除)。 */
export function safeExportName(topic: string) {
  const base = topic.replace(/[\\/:*?"<>|\r\n]+/g, '_').trim();
  return (base || '导出').slice(0, 60);
}
