import { useCallback, useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

/**
 * 可拖拽调宽的侧栏 hook(布局统一改造)。
 * 宽度缓存到 localStorage(key = panel-w:{key}),下次启动恢复。
 * 折叠态缓存到 localStorage(key = panel-collapsed:{key}),默认折叠。
 * direction:
 *  - 'left'  面板在左侧,把手在面板右缘,向右拖 = 变宽
 *  - 'right' 面板在右侧,把手在面板左缘,向左拖 = 变宽
 * 响应式:
 *  - 首次挂载时优先读取 CSS 栏宽 token(随断点降档/升档),读不到才用 def;
 *  - 宽度按视口比例封顶,窄窗下不会把主区挤到不可用(_tokens.css 只降档栏宽、不硬隐藏)。
 */
const KEY_PREFIX = 'panel-w:';
const COLLAPSED_PREFIX = 'panel-collapsed:';

/** 面板宽度默认值对应的 CSS token(与 _tokens.css 一致);未登记的面板用传入的 def。 */
const WIDTH_VARS: Record<string, string> = {
  mempanel: '--mempanel-w',
  wbinspector: '--wb-inspector-w',
  wbside: '--sidebar-w',
};

/** 单个面板最多占视口的比例:左右两侧同时展开时仍为主区留出空间。 */
const VIEWPORT_RATIO = 0.45;

/** 读取当前断点下该面板的 token 宽度(px);失败或越界时回退 def。 */
function readCssWidth(varName: string | undefined, def: number, min: number, max: number): number {
  if (!varName) return def;
  try {
    const raw = getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
    const n = Number.parseFloat(raw);
    if (Number.isFinite(n) && n > 0) return Math.min(max, Math.max(min, Math.round(n)));
  } catch {
    /* 非浏览器环境(测试)等:回退默认值 */
  }
  return def;
}

function readStored(key: string, def: number, min: number, max: number): number {
  try {
    const v = Number(localStorage.getItem(KEY_PREFIX + key));
    if (Number.isFinite(v) && v >= min && v <= max) return Math.round(v);
  } catch {
    /* localStorage 不可用时用默认值 */
  }
  return def;
}

function readCollapsed(key: string, def: boolean): boolean {
  try {
    const v = localStorage.getItem(COLLAPSED_PREFIX + key);
    if (v === '1') return true;
    if (v === '0') return false;
  } catch {
    /* ignore */
  }
  return def;
}

export interface PanelResize {
  /** 当前宽度(px),直接作为面板 inline width */
  width: number;
  /** 是否窄档(用于降字号/紧凑样式) */
  narrow: boolean;
  dragging: boolean;
  /** 是否已折叠(折叠时面板不渲染,仅保留展开入口) */
  collapsed: boolean;
  /** 切换折叠态并持久化 */
  toggleCollapse: () => void;
  /** 绑定到把手元素的 pointer 事件 */
  onHandleDown: (e: ReactPointerEvent) => void;
  onHandleMove: (e: ReactPointerEvent) => void;
  onHandleUp: (e: ReactPointerEvent) => void;
}

export function usePanelWidth(
  key: string,
  def: number,
  min: number,
  max: number,
  direction: 'left' | 'right' = 'left',
  narrowAt = Math.round(def * 0.85),
  defaultCollapsed = true,
): PanelResize {
  const [width, setWidth] = useState(() =>
    readStored(
      key,
      readCssWidth(WIDTH_VARS[key], def, min, max),
      min,
      max,
    ),
  );
  const [dragging, setDragging] = useState(false);
  const [collapsed, setCollapsed] = useState(() => readCollapsed(key, defaultCollapsed));
  /** 视口宽度(按 40px 量化,避免拖拽窗口时逐像素重渲染) */
  const [viewportW, setViewportW] = useState(() =>
    typeof window === 'undefined' ? 1280 : Math.round(window.innerWidth / 40) * 40,
  );
  const start = useRef({ x: 0, w: 0 });

  useEffect(() => {
    const onResize = () => setViewportW(Math.round(window.innerWidth / 40) * 40);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  /** 视口封顶后的实际可拖范围上限(窄窗下拖不到超出主区可承受的宽度) */
  const effMax = Math.max(min, Math.min(max, Math.round(viewportW * VIEWPORT_RATIO)));

  // 窗口变窄时回收超出视口比例的面板宽度,避免主区被挤到不可用
  useEffect(() => {
    setWidth((w) => Math.min(w, effMax));
  }, [effMax]);

  const toggleCollapse = useCallback(() => {
    setCollapsed((c) => {
      const next = !c;
      try {
        localStorage.setItem(COLLAPSED_PREFIX + key, next ? '1' : '0');
      } catch {
        /* ignore */
      }
      return next;
    });
  }, [key]);

  const onHandleDown = useCallback(
    (e: ReactPointerEvent) => {
      e.preventDefault();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      start.current = { x: e.clientX, w: width };
      setDragging(true);
    },
    [width],
  );

  const onHandleMove = useCallback(
    (e: ReactPointerEvent) => {
      if (!dragging) return;
      const dx = e.clientX - start.current.x;
      const raw = direction === 'left' ? start.current.w + dx : start.current.w - dx;
      setWidth(Math.min(effMax, Math.max(min, Math.round(raw))));
    },
    [dragging, direction, min, effMax],
  );

  const onHandleUp = useCallback(
    (e: ReactPointerEvent) => {
      if (!dragging) return;
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
      setDragging(false);
      setWidth((w) => {
        try {
          localStorage.setItem(KEY_PREFIX + key, String(w));
        } catch {
          /* ignore */
        }
        return w;
      });
    },
    [dragging, key],
  );

  return {
    width,
    narrow: width < narrowAt,
    dragging,
    collapsed,
    toggleCollapse,
    onHandleDown,
    onHandleMove,
    onHandleUp,
  };
}

// ---------- 思考/阶段卡高度拖拽(研究报告时间线) ----------

const CARD_H_KEY = 'rp-card-h';
const CARD_H_MIN = 88;
/** 上限:70% 视口高。 */
const CARD_H_MAX_RATIO = 0.7;
/** 全局卡高的 CSS 变量名(app.css 的 .rp-tl > .dd / .trail-card > .trail-body 引用)。 */
export const CARD_H_VAR = '--rp-card-h';

/** 把手所属卡片的可滚动正文(.rp-tl 的 .dd / .trail-card 的 .trail-body)。 */
function cardBodyOf(handle: HTMLElement): HTMLElement | null {
  const card = handle.closest('.rp-tl, .trail-card');
  return card ? card.querySelector(':scope > .dd, :scope > .trail-body') : null;
}

/**
 * 思考/阶段卡高度拖拽 hook:拖卡片底缘调 max-height,双击把手复位。
 * 高度为全局默认(所有卡共用),持久化 localStorage['rp-card-h'] 并写 CSS 变量
 * `--rp-card-h` 即时对全卡生效;全程直接操作 DOM/CSS 变量,不经 React state,
 * 流式输出期间拖拽零重渲染。
 */
export function useCardResize() {
  const st = useRef({ y: 0, h: 0, active: false });

  const onDown = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const body = cardBodyOf(e.currentTarget);
    if (!body) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    e.currentTarget.classList.add('dragging');
    st.current = { y: e.clientY, h: body.getBoundingClientRect().height, active: true };
  }, []);

  const onMove = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (!st.current.active) return;
    const max = Math.round(window.innerHeight * CARD_H_MAX_RATIO);
    const h = Math.min(
      max,
      Math.max(CARD_H_MIN, Math.round(st.current.h + e.clientY - st.current.y)),
    );
    document.documentElement.style.setProperty(CARD_H_VAR, `${h}px`);
  }, []);

  const onUp = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (!st.current.active) return;
    st.current.active = false;
    e.currentTarget.classList.remove('dragging');
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    const v = document.documentElement.style.getPropertyValue(CARD_H_VAR).trim();
    if (v) {
      try {
        localStorage.setItem(CARD_H_KEY, v);
      } catch {
        /* ignore */
      }
    }
  }, []);

  /** 双击复位:清 CSS 变量与持久化,所有卡恢复内容自适应。 */
  const onDoubleClick = useCallback(() => {
    document.documentElement.style.removeProperty(CARD_H_VAR);
    try {
      localStorage.removeItem(CARD_H_KEY);
    } catch {
      /* ignore */
    }
  }, []);

  // 挂载时恢复上次调整的全局高度
  useEffect(() => {
    try {
      const v = localStorage.getItem(CARD_H_KEY);
      if (v) document.documentElement.style.setProperty(CARD_H_VAR, v);
    } catch {
      /* ignore */
    }
  }, []);

  return { onDown, onMove, onUp, onDoubleClick };
}
