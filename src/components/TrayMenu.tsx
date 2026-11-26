import { useEffect, useLayoutEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useThemeStore } from '../stores/theme';
import '../styles/trayMenu.css';

/** 托盘自定义菜单动作:交给后端执行并收起菜单。 */
const act = (action: string) => {
  void invoke('tray_menu_action', { action }).catch(() => {});
};

/**
 * 托盘右键菜单(独立无边框透明窗口)。
 * 样式完全复用主窗口的 CSS 变量,因此切换主题后菜单同主题化。
 */
export default function TrayMenu() {
  const cardRef = useRef<HTMLDivElement>(null);
  const sent = useRef({ w: 0, h: 0 });

  // 菜单窗口按内容自适应:测量卡片实际尺寸 → 后端收缩窗口并重新贴住托盘图标。
  // 避免固定 216×152 窗口残留的透明留白(视觉上「偏上」且「过宽」)。
  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const fit = () => {
      const r = el.getBoundingClientRect();
      // 卡片可见尺寸 + 卡片在窗口内的偏移:后端按「卡片」而非窗口定位,
      // 这样窗口透明余量 / DPI 取整都不会让菜单与托盘图标之间出现空隙。
      const cw = Math.ceil(r.width);
      const ch = Math.ceil(r.height);
      const w = cw + 4;
      const h = ch + 4;
      if (Math.abs(w - sent.current.w) < 1 && Math.abs(h - sent.current.h) < 1) return;
      sent.current = { w, h };
      void invoke('tray_menu_fit', {
        width: w,
        height: h,
        cardWidth: cw,
        cardHeight: ch,
        offsetX: Math.round(r.left),
        offsetY: Math.round(r.top),
      }).catch(() => {});
    };
    fit();
    // 字体加载完成后字宽可能变化,再校准一次
    void document.fonts?.ready.then(fit);
    const timer = window.setTimeout(fit, 300);
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => {
      window.clearTimeout(timer);
      ro.disconnect();
    };
  }, []);

  useEffect(() => {
    // 菜单窗口同样挂载 data-theme:主题来自同一 localStorage
    const theme = useThemeStore.getState();
    theme.applyTheme(theme.theme);
    document.body.classList.add('tray-menu-window');
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') act('close');
    };
    // 双保险:后端失焦隐藏之外,前端也主动收起(防止个别平台不派发 Focused 事件)
    const onBlur = () => act('close');
    window.addEventListener('keydown', onKey);
    window.addEventListener('blur', onBlur);
    return () => {
      document.body.classList.remove('tray-menu-window');
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', onBlur);
    };
  }, []);

  return (
    <div className="tray-menu" role="menu" ref={cardRef}>
      <button type="button" role="menuitem" onClick={() => act('show')}>
        打开主窗口
      </button>
      <button type="button" role="menuitem" onClick={() => act('popout')}>
        小窗对话
      </button>
      <div className="tray-menu-sep" />
      <button type="button" role="menuitem" className="danger" onClick={() => act('quit')}>
        退出
      </button>
    </div>
  );
}
