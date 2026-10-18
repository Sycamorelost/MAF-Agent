import { useLocation, useNavigate } from 'react-router-dom';
import { IconChat, IconBrain, IconNote, IconAttach, IconResearch, IconImage, IconTranslate, IconSettings } from './icons';

/** 主导航(图标 + 迷你标签)。 */
const MAIN_ITEMS = [
  { key: 'chat', Icon: IconChat, label: '对话', path: '/' },
  { key: 'research', Icon: IconResearch, label: '研究', path: '/research' },
  { key: 'draw', Icon: IconImage, label: '绘图', path: '/draw' },
  { key: 'translate', Icon: IconTranslate, label: '翻译', path: '/translate' },
  { key: 'memory', Icon: IconBrain, label: '记忆', path: '/memory' },
  { key: 'notes', Icon: IconNote, label: '知识库', path: '/notes' },
  { key: 'attachments', Icon: IconAttach, label: '附件', path: '/attachments' },
];

/** 底部固定项(对齐模板:设置沉底)。 */
const FOOT_ITEMS = [
  { key: 'settings', Icon: IconSettings, label: '设置', path: '/settings' },
];

/** 有子路径前缀的用 startsWith 判断 active;首页精确匹配。 */
function isActive(path: string, pathname: string) {
  if (path === '/') return pathname === '/';
  return pathname.startsWith(path);
}

export default function Rail() {
  const nav = useNavigate();
  const loc = useLocation();

  const renderItem = ({ key, Icon, label, path }: (typeof MAIN_ITEMS)[number]) => (
    <button
      key={key}
      className={`rail-btn ${isActive(path, loc.pathname) ? 'active' : ''}`}
      title={label}
      onClick={() => nav(path)}
    >
      <Icon size={20} />
    </button>
  );

  return (
    <div className="rail">
      {MAIN_ITEMS.map(renderItem)}
      <div className="spacer" />
      {FOOT_ITEMS.map(renderItem)}
    </div>
  );
}
