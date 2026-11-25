import { useEffect } from 'react';
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
    <div className="tray-menu" role="menu">
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
