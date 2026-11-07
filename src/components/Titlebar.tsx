import { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useProvidersStore } from '../stores/providers';
import { useSessionsStore } from '../stores/sessions';
import { useUiStore } from '../stores/ui';
import { sessionPath } from '../lib/workbench';
import GlobalSearch from './GlobalSearch';
import AgentPickerDialog from './AgentPickerDialog';
import { IconEdit, IconSearch, IconX } from './icons';

/**
 * 自定义标题栏(简洁模式):
 * 左侧 ⋮ 导航菜单 + 玻璃组件组(会话抽屉/全局搜索/新建对话),
 * 右侧置顶/最小化/最大化/关闭;整条可拖拽移动窗口。
 */
export default function Titlebar() {
  const win = getCurrentWindow();
  const [pinned, setPinned] = useState(false);

  const nav = useNavigate();
  const loc = useLocation();
  const current = useProvidersStore((s) => s.current);
  const create = useSessionsStore((s) => s.create);
  const pruneEmpty = useSessionsStore((s) => s.pruneEmpty);

  const [searchOpen, setSearchOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);

  // 路由变化收起浮层(搜索跳转 / 导航后不残留);抽屉开启状态持久化,不随路由复位
  useEffect(() => {
    setSearchOpen(false);
  }, [loc.pathname]);

  // Ctrl/Cmd + K 打开搜索浮层(GlobalSearch 内部负责聚焦)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    void win.isAlwaysOnTop().then(setPinned).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onMinimize = () => {
    // 普通最小化:窗口留在任务栏;只有「关闭」才按设置隐藏到托盘
    void win.minimize();
  };

  const onTogglePin = () => {
    const next = !pinned;
    setPinned(next);
    void win.setAlwaysOnTop(next).catch(() => setPinned(!next));
  };

  // 新建对话:先选智能体再建会话(原 Topbar 逻辑迁入)
  const onSelectAgent = async (agentId: string | null) => {
    setPickerOpen(false);
    // 空对话默认不保留:新建前先清掉遗留的空会话
    void pruneEmpty(60);
    const title = `新会话 ${new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    // 统一为 provider/model 格式,便于按会话回填模型
    const modelId = current ? `${current.providerId}/${current.model}` : '';
    const id = await create(title, modelId, agentId);
    nav(sessionPath(id));
  };

  return (
    <div className="titlebar" data-tauri-drag-region>
      {/* 玻璃组件组:侧栏折叠 / 全局搜索 / 新建对话(导航已移入侧栏) */}
      <div className="tb-pill">
        <button
          className={`tb-pill-btn${sidebarOpen ? ' on' : ''}`}
          title={sidebarOpen ? '折叠侧栏' : '展开侧栏'}
          aria-label={sidebarOpen ? '折叠侧栏' : '展开侧栏'}
          aria-pressed={sidebarOpen}
          onClick={() => useUiStore.getState().setSidebarOpen(!sidebarOpen)}
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3">
            <rect x="1.8" y="2.5" width="12.4" height="11" rx="2" />
            <path d="M6 2.5v11" />
          </svg>
        </button>
        <button className="tb-pill-btn" title="搜索 (Ctrl+K)" aria-label="搜索 (Ctrl+K)" onClick={() => setSearchOpen((v) => !v)}>
          <IconSearch size={15} />
        </button>
        <button className="tb-pill-btn" title="新建对话" aria-label="新建对话" onClick={() => setPickerOpen(true)}>
          <IconEdit size={15} />
        </button>
      </div>

      <span className="spacer" data-tauri-drag-region />

      <button
        className={`win-btn${pinned ? ' pinned' : ''}`}
        title={pinned ? '取消置顶' : '置顶'}
        aria-label={pinned ? '取消置顶' : '置顶'}
        onClick={onTogglePin}
      >
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M7 1.5l3.5 3.5-2 .5L6 8l-.5 2.5L3 8 1.5 6.5 4 6l2.5-2.5.5-2z" />
        </svg>
      </button>
      <button className="win-btn" title="最小化" aria-label="最小化" onClick={onMinimize}>
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M2 6h8" />
        </svg>
      </button>
      <button className="win-btn" title="最大化/还原" aria-label="最大化/还原" onClick={() => void win.toggleMaximize()}>
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <rect x="2.5" y="2.5" width="7" height="7" rx="1" />
        </svg>
      </button>
      <button className="win-btn close" title="关闭" aria-label="关闭" onClick={() => void win.close()}>
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M3 3l6 6M9 3l-6 6" />
        </svg>
      </button>

      {/* 全局搜索浮层(玻璃卡片,挂在 pill 下方) */}
      {searchOpen && (
        <>
          <div className="tb-pop-backdrop" onClick={() => setSearchOpen(false)} />
          <div className="tb-search-pop">
            <div className="tb-search-head">
              <span>搜索</span>
              <button className="panel-toggle" title="关闭" aria-label="关闭搜索" onClick={() => setSearchOpen(false)}>
                <IconX size={13} />
              </button>
            </div>
            <GlobalSearch />
          </div>
        </>
      )}

      {pickerOpen && (
        <AgentPickerDialog
          currentAgentId={null}
          onSelect={(id) => void onSelectAgent(id)}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}
