import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { SetURLSearchParams } from 'react-router-dom';
import type { VirtualizerHandle } from 'virtua';
import type { MessageRow } from '../../lib/ipc';
import { useMessagesStore } from '../../stores/messages';

interface ChatPageScrollParams {
  sessionId: string | undefined;
  messages: MessageRow[];
  hasMore: boolean;
  loadOlder: (sessionId: string) => Promise<void>;
  searchParams: URLSearchParams;
  setSearchParams: SetURLSearchParams;
}

/** 会话页滚动与定位:虚拟列表偏移测量、分页前插保持位置、全局搜索/纲要跳转、
 *  以及「跟随底部」自动滚动。 */
export function useChatPageScroll({
  sessionId,
  messages,
  hasMore,
  loadOlder,
  searchParams,
  setSearchParams,
}: ChatPageScrollParams) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<VirtualizerHandle | null>(null);
  const listWrapRef = useRef<HTMLDivElement>(null);
  /** P3:虚拟列表在滚动容器内的起始偏移(上方横幅/任务清单等兄弟节点占位) */
  const [listMargin, setListMargin] = useState(0);

  /** 前插历史标记:加载更早一页时抑制「自动滚到底部」,避免把视口甩走 */
  const prependingRef = useRef(false);

  /** 加载更早一页并保持视口位置(Virtualizer shift 以列表尾部为锚点自动补偿高度差) */
  const onLoadOlder = async () => {
    if (!sessionId) return;
    if (!useMessagesStore.getState().hasMore[sessionId]) return;
    prependingRef.current = true;
    await loadOlder(sessionId);
  };

  // ---------- P3:虚拟列表起始偏移测量(横幅/任务清单等上方兄弟节点高度会动态变化) ----------
  useLayoutEffect(() => {
    const measure = () => {
      const wrap = listWrapRef.current;
      const sc = scrollRef.current;
      if (!wrap || !sc) return;
      const m = Math.max(
        0,
        Math.round(wrap.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop),
      );
      setListMargin((prev) => (Math.abs(prev - m) <= 1 ? prev : m));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    const sc = scrollRef.current;
    if (sc) {
      ro.observe(sc);
      if (sc.firstElementChild) ro.observe(sc.firstElementChild);
    }
    return () => ro.disconnect();
  }, [sessionId]);

  // ---------- 全局搜索跳转定位 ----------
  const locatingRef = useRef(false);
  useEffect(() => {
    locatingRef.current = false;
  }, [sessionId, searchParams]);
  useEffect(() => {
    const target = searchParams.get('msg');
    if (!target || messages.length === 0) return;
    const idx = messages.findIndex((m) => String(m.id) === target);
    if (idx >= 0) {
      // P3:虚拟列表只挂载视口附近消息,离屏目标用 index 定位;
      // 定位后等待条目挂载再闪烁高亮(高度测量可能需几帧)
      listRef.current?.scrollToIndex(idx, { align: 'center' });
      let attempts = 0;
      let raf = 0;
      let timer = 0;
      const highlight = () => {
        const el = document.getElementById(`msg-${target}`);
        if (!el) {
          if (++attempts < 12) {
            raf = requestAnimationFrame(highlight);
          } else {
            setSearchParams({}, { replace: true });
          }
          return;
        }
        el.scrollIntoView({ block: 'center' });
        el.classList.add('msg-flash');
        timer = window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
        setSearchParams({}, { replace: true });
      };
      raf = requestAnimationFrame(highlight);
      return () => {
        cancelAnimationFrame(raf);
        if (timer) window.clearTimeout(timer);
      };
    }
    // 目标消息在更早分页里:继续向前加载,直到命中(或已无更早消息)
    if (hasMore && sessionId) {
      if (!locatingRef.current) {
        locatingRef.current = true;
        void loadOlder(sessionId);
      }
      return;
    }
    // 目标消息不存在(已被删除):清掉定位参数
    setSearchParams({}, { replace: true });
  }, [searchParams, messages, setSearchParams, hasMore, sessionId, loadOlder]);

  /** 纲要跳转:定位到消息首行;虚拟列表只挂载视口附近消息,先按 index 滚动,待条目挂载后再 flash。 */
  const onOutlineJump = useCallback(
    (msgId: number) => {
      const idx = messages.findIndex((m) => m.id === msgId);
      if (idx < 0) return;
      listRef.current?.scrollToIndex(idx, { align: 'start' });
      let attempts = 0;
      const highlight = () => {
        const el = document.getElementById(`msg-${msgId}`);
        if (!el) {
          if (++attempts < 12) requestAnimationFrame(highlight);
          return;
        }
        el.scrollIntoView({ block: 'start' });
        el.classList.add('msg-flash');
        window.setTimeout(() => el.classList.remove('msg-flash'), 1800);
      };
      requestAnimationFrame(highlight);
    },
    [messages],
  );

  // 新消息到达 / 切换会话时:平滑滚动到底部
  // (?msg 定位期间不抢滚动,否则会把 scrollIntoView 的目标甩到底部;
  //  用 ref 读取最新目标,避免定位完成后清空 query 触发本 effect 再次拉到底部)
  const msgTargetRef = useRef<string | null>(null);
  msgTargetRef.current = searchParams.get('msg');
  /** 用户是否停在底部(跟随新内容);一旦手动上滚即暂停,滚回底部恢复。 */
  const followRef = useRef(true);
  /** 换会话时重置为跟随 */
  useEffect(() => {
    followRef.current = true;
  }, [sessionId]);
  useEffect(() => {
    if (prependingRef.current) {
      prependingRef.current = false;
      return;
    }
    if (msgTargetRef.current) return;
    // 用户正在上滚查看时不抢滚动,避免「一有输出就被拽回底部」
    if (!followRef.current) return;
    // P3:虚拟列表条目按需挂载,直接 scrollTo(scrollHeight) 会因高度估算偏短;
    // 先按 index 定位到最后一条(库内部会在测量后校正),再补滚动一次以覆盖列表下方元素。
    const n = messages.length;
    if (n > 0) listRef.current?.scrollToIndex(n - 1, { align: 'end' });
    const sc = scrollRef.current;
    const raf = requestAnimationFrame(() => {
      sc?.scrollTo({ top: sc.scrollHeight, behavior: 'smooth' });
    });
    return () => cancelAnimationFrame(raf);
  }, [sessionId, messages.length]);

  /** 距底部 80px 内视为「跟随」,否则用户已主动上滚查看。 */
  const onScrollFollow = () => {
    const sc = scrollRef.current;
    if (!sc) return;
    followRef.current = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  };

  /** 用户主动发送:恢复「跟随到底部」 */
  const resumeFollow = useCallback(() => {
    followRef.current = true;
  }, []);

  return {
    scrollRef,
    listRef,
    listWrapRef,
    listMargin,
    onLoadOlder,
    onScrollFollow,
    onOutlineJump,
    resumeFollow,
  };
}
