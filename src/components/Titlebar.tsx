import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import logoUrl from '../assets/logo.png';
import * as ipc from '../lib/ipc';

/** 自定义标题栏:无边框窗口下替代系统标题栏,拖拽移动 + 置顶/最小化/最大化/关闭。 */
export default function Titlebar() {
  const win = getCurrentWindow();
  const [pinned, setPinned] = useState(false);
  /** 最小化是否进托盘(随设置热更新,默认关) */
  const minToTray = useRef(false);

  useEffect(() => {
    void win.isAlwaysOnTop().then(setPinned).catch(() => {});
    void ipc.getSettings().then((s) => { minToTray.current = s.minimize_to_tray; }).catch(() => {});
    const un = listen('settings_changed', () => {
      void ipc.getSettings().then((s) => { minToTray.current = s.minimize_to_tray; }).catch(() => {});
    });
    return () => { void un.then((f) => f()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onMinimize = () => {
    // 进托盘:直接 hide;否则常规最小化(后端 Resized 监听兜底其他最小化路径)
    if (minToTray.current) void win.hide();
    else void win.minimize();
  };

  const onTogglePin = () => {
    const next = !pinned;
    setPinned(next);
    void win.setAlwaysOnTop(next).catch(() => setPinned(!next));
  };

  return (
    <div className="titlebar" data-tauri-drag-region>
      <img className="logo" src={logoUrl} alt="" data-tauri-drag-region />
      <span className="appname" data-tauri-drag-region>
        MAF Agent
      </span>
      <span className="spacer" data-tauri-drag-region />
      <button
        className={`win-btn${pinned ? ' pinned' : ''}`}
        title={pinned ? '取消置顶' : '置顶'}
        onClick={onTogglePin}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M7 1.5l3.5 3.5-2 .5L6 8l-.5 2.5L3 8 1.5 6.5 4 6l2.5-2.5.5-2z" />
        </svg>
      </button>
      <button className="win-btn" title="最小化" onClick={onMinimize}>
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M2 6h8" />
        </svg>
      </button>
      <button className="win-btn" title="最大化/还原" onClick={() => void win.toggleMaximize()}>
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <rect x="2.5" y="2.5" width="7" height="7" rx="1" />
        </svg>
      </button>
      <button className="win-btn close" title="关闭" onClick={() => void win.close()}>
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M3 3l6 6M9 3l-6 6" />
        </svg>
      </button>
    </div>
  );
}
