import { useEffect, useState } from 'react';

/** 用 IntersectionObserver 监听消息列表,返回当前滚动区域内最醒目的消息 id。 */
export function useActiveMessageId(messageIds: (number | string)[]): number | null {
  const [activeId, setActiveId] = useState<number | null>(null);

  useEffect(() => {
    if (messageIds.length === 0) return;

    const scroller = document.querySelector('.chat-scroll');
    if (!scroller) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => b.intersectionRatio - a.intersectionRatio);
        if (visible[0]) {
          const raw = visible[0].target.id.replace(/^msg-/, '');
          const n = Number(raw);
          if (!Number.isNaN(n)) setActiveId(n);
        }
      },
      { root: scroller, threshold: [0.1, 0.3, 0.5, 0.7] },
    );

    messageIds.forEach((id) => {
      const el = document.getElementById(`msg-${id}`);
      if (el) observer.observe(el);
    });

    return () => observer.disconnect();
  }, [messageIds]);

  return activeId;
}
