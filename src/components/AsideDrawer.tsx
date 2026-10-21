import { useEffect, type ReactNode } from 'react';
import { useUiStore } from '../stores/ui';
import { IconX } from './icons';

/**
 * 页面左栏玻璃抽屉(布局统一改造):
 * 各页侧栏默认收起,由标题栏玻璃 pill 的「面板」按钮释放;
 * 打开后持久保持(挤出式:主区自动让位),点 pill / ✕ / Esc 关闭。
 * 抽屉为 300px 定宽玻璃浮层,不占据布局列。
 */
export default function AsideDrawer({ children }: { children: ReactNode }) {
  const open = useUiStore((s) => s.asideOpen);
  const setAsideOpen = useUiStore((s) => s.setAsideOpen);

  // 挤出式:挂 body class 让主区让位;卸载/关闭时移除
  useEffect(() => {
    document.body.classList.toggle('aside-open', open);
    return () => document.body.classList.remove('aside-open');
  }, [open]);

  // Esc 关闭抽屉
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setAsideOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, setAsideOpen]);

  if (!open) return null;

  return (
    <>
      {children}
      <button className="panel-toggle aside-close" title="关闭面板" onClick={() => setAsideOpen(false)}>
        <IconX size={14} />
      </button>
    </>
  );
}
