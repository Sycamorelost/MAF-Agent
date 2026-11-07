import { useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import * as ipc from '../../lib/ipc';
import { isNavActive } from './utils';
import { IconAttach, IconBrain, IconChat, IconFileText, IconSettings } from '../icons';

/** 一级导航(设置固定在最底部,不参与排序与隐藏)。 */
const NAV_MAIN = [
  { key: 'chat', Icon: IconChat, label: '对话', path: '/' },
  { key: 'memory', Icon: IconBrain, label: '记忆库', path: '/memory' },
  { key: 'knowledge', Icon: IconFileText, label: '知识库', path: '/knowledge' },
  { key: 'attachments', Icon: IconAttach, label: '附件', path: '/attachments' },
];

const NAV_FOOT = [{ key: 'settings', Icon: IconSettings, label: '设置', path: '/settings' }];

interface Props {
  /** 只渲染底部导航(设置) */
  foot?: boolean;
}

/** 侧栏导航区:折叠时整个侧栏不渲染。 */
export function NavRail({ foot = false }: Props) {
  const nav = useNavigate();
  const loc = useLocation();
  /** 设置中被隐藏的功能项(对话/设置固定保留) */
  const [hidden, setHidden] = useState<string[]>([]);
  /** 设置中自定义的显示顺序(未列出的项按内置顺序补在末尾) */
  const [order, setOrder] = useState<string[]>([]);

  useEffect(() => {
    if (foot) return;
    const apply = (s: ipc.AppSettings) => {
      setHidden(s.hidden_nav_items ?? []);
      setOrder(s.nav_order ?? []);
    };
    void ipc.getSettings().then(apply).catch(() => {});
    const un = listen<ipc.AppSettings>('settings_changed', (e) => apply(e.payload));
    return () => {
      void un.then((f) => f());
    };
  }, [foot]);

  /** 按用户自定义顺序重排;「对话」固定第一。 */
  const items = useMemo(() => {
    if (foot) return NAV_FOOT;
    const rank = new Map(order.map((k, i) => [k, i]));
    return NAV_MAIN.filter((i) => !hidden.includes(i.key)).sort((a, b) => {
      if (a.key === 'chat') return -1;
      if (b.key === 'chat') return 1;
      return (
        (rank.get(a.key) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.key) ?? Number.MAX_SAFE_INTEGER)
      );
    });
  }, [foot, hidden, order]);

  return (
    <nav className={`nav-rail${foot ? ' foot' : ''}`} aria-label={foot ? '底部导航' : '主导航'}>
      {items.map(({ key, Icon, label, path }) => {
        const active = isNavActive(path, loc.pathname, loc.search);
        return (
          <button
            key={key}
            className={`nav-item${active ? ' active' : ''}`}
            aria-label={label}
            aria-current={active ? 'page' : undefined}
            onClick={() => nav(path)}
          >
            <Icon size={16} />
            <span>{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
