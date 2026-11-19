import { useEffect, useState } from 'react';
import type { RefObject } from 'react';

/** 划词选中浮动工具栏:文本 + 屏幕坐标(相对视口,Portal 到 body 直接用) + 来源消息 id */
export interface ChatSelectionTool {
  text: string;
  x: number;
  y: number;
  msgId?: number;
}

/**
 * 划词选中检测(mouseup 检测选区,定位到选区上方)。
 * 只在消息区(listWrapRef 内)选中非空文本时弹工具栏。
 */
export function useChatSelectionToolbar(listWrapRef: RefObject<HTMLDivElement | null>) {
  const [selTool, setSelTool] = useState<ChatSelectionTool | null>(null);

  useEffect(() => {
    const onMouseUp = () => {
      const sel = window.getSelection();
      const text = sel?.toString().trim() ?? '';
      // 只在消息区(listWrapRef 内)选中非空文本时弹工具栏
      if (!text || !sel || sel.isCollapsed) { setSelTool(null); return; }
      const range = sel.getRangeAt(0);
      const rect = range.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) { setSelTool(null); return; }
      // 确保选区在消息列表容器内(不是 Composer 或其他区域)
      const list = listWrapRef.current;
      if (list && !list.contains(range.commonAncestorContainer)) { setSelTool(null); return; }
      // 向上找最近的 [data-msg-id] 容器,拿到来源消息 id
      let node: Node | null = range.commonAncestorContainer;
      let msgId: number | undefined;
      while (node) {
        if (node.nodeType === 1) {
          const el = node as HTMLElement;
          const idStr = el.getAttribute?.('data-msg-id');
          if (idStr) { msgId = Number(idStr); break; }
        }
        node = node.parentNode;
      }
      setSelTool({
        text: text.slice(0, 500), // 截断防极长
        x: rect.left + rect.width / 2,
        y: rect.top - 8,
        msgId,
      });
    };
    const onMouseDown = (e: MouseEvent) => {
      // 点到工具栏自身时不关闭
      const t = e.target as HTMLElement;
      if (t?.closest?.('.sel-float-tool')) return;
      setSelTool(null);
    };
    document.addEventListener('mouseup', onMouseUp);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      document.removeEventListener('mouseup', onMouseUp);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [listWrapRef]);

  return { selTool, setSelTool };
}
