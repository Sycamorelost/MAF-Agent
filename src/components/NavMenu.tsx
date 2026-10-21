import { useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import * as ipc from '../lib/ipc';
import {
  IconAttach,
  IconBrain,
  IconChat,
  IconGraduation,
  IconImage,
  IconNote,
  IconResearch,
  IconSettings,
  IconTranslate,
} from './icons';

/** 原图标轨(Rail)的导航项,收进标题栏 ⋮ 下拉菜单。 */
const MAIN_ITEMS = [
  { key: 'chat', Icon: IconChat, label: '对话', path: '/' },
  { key: 'research', Icon: IconResearch, label: '研究', path: '/research' },
  { key: 'plan', Icon: IconGraduation, label: '学习规划', path: '/plan' },
  { key: 'draw', Icon: IconImage, label: '绘图', path: '/draw' },
  { key: 'translate', Icon: IconTranslate, label: '翻译', path: '/translate' },
  { key: 'memory', Icon: IconBrain, label: '记忆', path: '/memory' },
  { key: 'notes', Icon: IconNote, label: '知识库', path: '/notes' },
  { key: 'attachments', Icon: IconAttach, label: '附件', path: '/attachments' },
];

const FOOT_ITEMS = [{ key: 'settings', Icon: IconSettings, label: '设置', path: '/settings' }];

/** 有子路径前缀的用 startsWith 判断 active;首页精确匹配。 */
function isActive(path: string, pathname: string) {
  if (path === '/') return pathname === '/';
  return pathname.startsWith(path);
}

/** 标题栏左侧的三点菜单按钮 + 导航下拉(替代原左侧图标轨)。 */
export function NavMenu() {
  const nav = useNavigate();
  const loc = useLocation();
  const [open, setOpen] = useState(false);
  /** 设置中被隐藏的功能项(对话/设置固定保留) */
  const [hidden, setHidden] = useState<string[]>([]);
  const rootRef = useRef<HTMLDivElement>(null);

  // 读取导航可见性设置,并随设置保存热更新
  useEffect(() => {
    const apply = (s: ipc.AppSettings) => setHidden(s.hidden_nav_items ?? []);
    void ipc.getSettings().then(apply).catch(() => {});
    const un = listen<ipc.AppSettings>('settings_changed', (e) => apply(e.payload));
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // 路由变化即收起;点击菜单外区域收起
  useEffect(() => setOpen(false), [loc.pathname]);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  const renderItem = ({ key, Icon, label, path }: (typeof MAIN_ITEMS)[number]) => (
    <button
      key={key}
      className={`nav-item${isActive(path, loc.pathname) ? ' active' : ''}`}
      onClick={() => nav(path)}
    >
      <Icon size={16} />
      <span>{label}</span>
    </button>
  );

  return (
    <div className="nav-menu" ref={rootRef}>
      <button
        className={`menu-btn${open ? ' on' : ''}`}
        title="导航菜单"
        aria-label="导航菜单"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor">
          <circle cx="8" cy="3" r="1.4" />
          <circle cx="8" cy="8" r="1.4" />
          <circle cx="8" cy="13" r="1.4" />
        </svg>
      </button>
      {open && (
        <div className="nav-pop">
          {MAIN_ITEMS.filter((i) => !hidden.includes(i.key)).map(renderItem)}
          <div className="nav-sep" />
          {FOOT_ITEMS.map(renderItem)}
        </div>
      )}
    </div>
  );
}
