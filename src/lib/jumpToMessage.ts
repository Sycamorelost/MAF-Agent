/** 平滑滚动到指定消息 DOM id,并触发 flash 高亮。 */
export function jumpToMessage(msgId: number) {
  const scroller = document.querySelector('.chat-scroll');
  const target = document.getElementById(`msg-${msgId}`);
  if (!scroller || !target) return;
  target.scrollIntoView({ behavior: 'smooth', block: 'center' });
  target.classList.add('msg-flash');
  setTimeout(() => target.classList.remove('msg-flash'), 1600);
}
