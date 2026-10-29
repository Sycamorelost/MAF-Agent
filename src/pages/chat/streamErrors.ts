/** 统一取错误文案:Error 取 message,其余转 String(空值也安全)。 */
export function toErrorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 发送失败文案:会话被空对话清理(外键失败)时提示重建,其余包装为「发送失败」。 */
export function describeSendError(msg: string): string {
  // 会话在发送过程中被删除(如空会话清理竞态),提示用户重建
  if (msg.includes('FOREIGN KEY constraint failed')) {
    return '会话已被清理，请返回首页重新创建对话';
  }
  return `发送失败: ${msg}`;
}
