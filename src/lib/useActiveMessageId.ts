import { useEffect, useRef, useState } from 'react';

/**
 * 用 IntersectionObserver + 实时 scroll 监听,返回当前滚动区域内**最接近屏幕中心**的消息 id。
 * 之前只按 intersectionRatio 排序会有 bug(多条底部消息都 100% 可见时排序不稳定),
 * 现在改用「消息中心 → scroller 视口中心」的距离排序,滚动到哪条就高亮哪条,灵敏稳定。
 */
export function useActiveMessageId(messageIds: (number | string)[]): number | null {
  const [activeId, setActiveId] = useState<number | null>(null);
  const observerRef = useRef<IntersectionObserver | null>(null);
  const scrollerRef = useRef<Element | null>(null);
  /** 当前进入过视口的消息 id 集合 */
  const visibleIds = useRef<Set<number>>(new Set());

  useEffect(() => {
    if (messageIds.length === 0) return;

    const scroller = document.querySelector('.chat-scroll');
    if (!scroller) return;
    scrollerRef.current = scroller;

    const pickActive = () => {
      const scrollRect = scroller.getBoundingClientRect();
      const viewCenter = scrollRect.top + scrollRect.height / 2;

      let bestId: number | null = null;
      let bestDist = Infinity;
      for (const id of visibleIds.current) {
        const el = document.getElementById(`msg-${id}`);
        if (!el) continue;
        const rect = el.getBoundingClientRect();
        const msgCenter = rect.top + rect.height / 2;
        const dist = Math.abs(msgCenter - viewCenter);
        if (dist < bestDist) {
          bestDist = dist;
          bestId = id;
        }
      }
      if (bestId !== null) setActiveId(bestId);
    };

    const observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const idRaw = Number(e.target.id.replace(/^msg-/, ''));
          if (Number.isNaN(idRaw)) continue;
          if (e.isIntersecting) {
            visibleIds.current.add(idRaw);
          } else {
            visibleIds.current.delete(idRaw);
          }
        }
        pickActive();
      },
      { root: scroller, threshold: [0, 0.05, 0.2, 0.5, 0.8, 1] },
    );
    observerRef.current = observer;

    messageIds.forEach((id) => {
      const el = document.getElementById(`msg-${id}`);
      if (el) observer.observe(el);
    });

    // scroll 事件里也实时 pick:IntersectionObserver 只在可见性变化时 fire,
    // 同一条消息还在视口内但滚动位置变了时,需要 scroll 事件重新算
    const onScroll = () => pickActive();
    scroller.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);

    // 初始化一次
    pickActive();

    return () => {
      observer.disconnect();
      scroller.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
      visibleIds.current.clear();
    };
  }, [messageIds]);

  return activeId;
}
