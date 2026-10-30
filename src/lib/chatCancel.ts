/** 会话流式取消的窗口事件总线:RunStatusBar / StatusBar 发出,useChatSend 监听执行。 */
export const CHAT_CANCEL_EVENT = 'chat-cancel-request';

/** 请求取消本会话当前流式(与 Composer 停止同一入口)。 */
export function requestChatCancel(sessionId: string) {
  window.dispatchEvent(new CustomEvent(CHAT_CANCEL_EVENT, { detail: sessionId }));
}
