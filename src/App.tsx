import { HashRouter, Routes, Route, Navigate, useNavigate, useParams, useLocation } from 'react-router-dom';
import { lazy, Suspense, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import Titlebar from './components/Titlebar';
import Sidebar from './components/sidebar/Sidebar';
import DataDirSetupDialog from './components/DataDirSetupDialog';
import OnboardingDialog from './components/OnboardingDialog';
import * as ipc from './lib/ipc';
import McpApprovalDialog from './components/McpApprovalDialog';
import { ConfirmHost } from './components/confirmService';
import TrayMenu from './components/TrayMenu';
import { getBootstrapState } from './lib/ipc/mcp';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useSessionsStore } from './stores/sessions';
import { useProvidersStore } from './stores/providers';
import { useThemeStore } from './stores/theme';
import { sessionPath } from './lib/workbench';
import './styles/base.css';
import './styles/dialog.css';
import './styles/home.css';
import './styles/chat.css';
import './styles/phase.css';
import './styles/tools.css';
import './styles/memory.css';
import './styles/settings.css';
import './styles/popout.css';
import './styles/workbench.css';
import 'katex/dist/katex.min.css';

// PERF-4:页面按路由懒加载,首屏只加载布局与当前页,避免 8 MB+ 单包一次性解析。
const HomePage = lazy(() => import('./pages/HomePage'));
const ChatPage = lazy(() => import('./pages/ChatPage'));
const MemoryPage = lazy(() => import('./pages/MemoryPage'));
const NotesPage = lazy(() => import('./pages/NotesPage'));
const AttachmentsPage = lazy(() => import('./pages/AttachmentsPage'));
const SettingsPage = lazy(() => import('./pages/SettingsPage'));

/** 首页门:工作台页已并入会话页,首页永远是 HomePage。 */
function HomeGate() {
  return <HomePage />;
}

/** 会话门(P3-1 路由收敛):仅做存在性校验;模式不再决定路由,单一会话页承载两种形态。 */
function SessionGate() {
  const { id } = useParams<{ id: string }>();
  const sessions = useSessionsStore((s) => s.sessions);
  const loaded = useSessionsStore((s) => s.loaded);
  if (!id) return <Navigate to="/" replace />;
  const session = sessions.find((s) => s.id === id);
  // 会话不存在且列表已加载完成(如删除后从历史栈返回):回首页,避免落在空白会话页
  if (!session && loaded) return <Navigate to="/" replace />;
  return <ChatPage />;
}

/** 数据目录引导门:未配置数据目录且非开发构建时,强制先选目录才能使用。 */
function DataDirGate() {
  const [need, setNeed] = useState(false);
  useEffect(() => {
    ipc
      .getDataDir()
      .then((i) => setNeed(!i.configured && !i.is_dev))
      .catch(() => {});
  }, []);
  if (!need) return null;
  return <DataDirSetupDialog onDone={() => setNeed(false)} />;
}

/** 首次启动向导门:未完成首次引导时显示底层 Prompt 确认向导。 */
function OnboardingGate() {
  const [need, setNeed] = useState(false);
  useEffect(() => {
    ipc
      .getSettings()
      .then((s) => setNeed(!s.first_run_completed))
      .catch(() => {});
  }, []);
  if (!need) return null;
  return <OnboardingDialog onDone={() => setNeed(false)} />;
}

/** Popout 小窗简化内容:无 Sidebar / Titlebar / Inspector,只留消息区 + Composer。
 *  内部用 CSS 隐藏 ChatPage 的额外面板,复用同一套会话/Send/Stream 逻辑。 */
function PopoutApp() {
  const nav = useNavigate();
  const [err, setErr] = useState<string | null>(null);
  // popout 一启动就创建新会话、跳到 /chat/:id、定位到右下角贴任务栏
  useEffect(() => {
    let disposed = false;
    (async () => {
      try {
        // 窗口创建时已在 Rust 端锚定右下角;这里再调一次作为防御(幂等,不产生跳位)
        await invoke('dock_popout').catch(() => {});
        const { id } = await ipc.createSession('', '');
        if (disposed) return;
        // 让 useSessionsStore 也拿到这个新会话(同 DB,主窗口也能看到)
        await useSessionsStore.getState().refresh();
        if (!disposed) nav(`/chat/${id}`, { replace: true });
      } catch (e) {
        if (!disposed) setErr(String(e ?? '新建会话失败'));
      }
    })();
    return () => { disposed = true; };
    // 仅 mount 时跑一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="popout-root">
      {/* 自定义标题栏:关闭按钮 */}
      <div className="popout-cap" data-tauri-drag-region>
        <span className="popout-title">myAgent</span>
        <button
          className="popout-x"
          title="关闭小窗(Esc)"
          onClick={() => void invoke<boolean>('toggle_popout').catch(() => {})}
        >×</button>
      </div>
      {/* 内容区:复用 ChatPage,但通过 CSS 隐藏 Sidebar/Titlebar/Inspector */}
      <div className="popout-body">
        {err ? (
          <div className="popout-center">{err}</div>
        ) : (
          <Suspense fallback={<div className="popout-center">加载中…</div>}>
            <Routes>
              <Route path="/chat/:id" element={<PopoutSessionGate />} />
              <Route path="*" element={<div className="popout-center">正在新建会话…</div>} />
            </Routes>
          </Suspense>
        )}
      </div>
    </div>
  );
}

/** Popout 专用会话门:不 Navigate 回根(会落入白屏),直接渲染 ChatPage;
 *  sessions 未就绪时 ChatPage 内部有自己的空状态处理。 */
function PopoutSessionGate() {
  const { id } = useParams<{ id: string }>();
  if (!id) return <ChatPage compactTools />;
  return <ChatPage key={id} compactTools />;
}

/** 主窗口内容:监听「跳转会话」/「打开设置页」事件并导航。 */
function MainApp() {
  const nav = useNavigate();
  const loc = useLocation();
  /** 主面板切换动画的分组键:一级路由变化才换场(会话之间切换不重播动画) */
  const viewKey = loc.pathname.split('/')[1] || 'home';
  useEffect(() => {
    const unlistenSession = listen<{ sessionId: string }>('navigate-to-session', (e) => {
      // 先刷新会话列表:目标会话可能由其它窗口刚创建,否则模式门会因「本地不存在」而回首页
      void useSessionsStore
        .getState()
        .refresh()
        .catch(() => {})
        .finally(() => nav(sessionPath(e.payload.sessionId)));
    });
    const unlistenSettings = listen<{ tab?: string }>('navigate-to-settings', (e) => {
      nav(e.payload.tab ? `/settings?tab=${e.payload.tab}` : '/settings');
    });
    // 外部编辑器改 memory.md frontmatter 的 conversation → 后端同步 sessions.title 后广播此事件
    const unlistenRenamed = listen('session_renamed', () => {
      void useSessionsStore.getState().refresh();
    });
    return () => {
      unlistenSession.then((f) => f());
      unlistenSettings.then((f) => f());
      unlistenRenamed.then((f) => f());
    };
  }, [nav]);

  /** 全局 Esc:关闭最上层可关闭浮层(等价于点遮罩,避免所有弹窗都各写一遍) */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      const layers = document.querySelectorAll<HTMLElement>('.dialog-mask, .draw-lightbox, .att-lightbox');
      layers[layers.length - 1]?.click();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  /** 启动时挂载 data-theme(从 localStorage 读取的初始主题),保证首屏即应用主题色。 */
  useEffect(() => {
    useThemeStore.getState().applyTheme(useThemeStore.getState().theme);
  }, []);

  return (
    <div className="window">
      <Titlebar />
      <div className="body">
        <Sidebar />
        <div className="main">
          <Suspense fallback={null}>
            <div className="main-view" key={viewKey}>
              <Routes>
                <Route path="/" element={<HomeGate />} />
                <Route path="/chat/:id" element={<SessionGate />} />
                {/* 工作台页已并入会话页,/work 路由不再注册;旧 /work/:id 链接直接由通配回首页 */}
                <Route path="/memory" element={<MemoryPage />} />
                {/* 知识库独立入口(侧栏「知识库」);旧 /notes 深链保留(?id= 直接定位笔记) */}
                <Route path="/knowledge" element={<NotesPage />} />
                <Route path="/notes" element={<NotesPage />} />
                <Route path="/attachments" element={<AttachmentsPage />} />
                <Route path="/settings" element={<SettingsPage />} />
                <Route path="*" element={<Navigate to="/" replace />} />
              </Routes>
            </div>
          </Suspense>
        </div>
      </div>
    </div>
  );
}

/** 首启预热提示:后端自启 MCP 组件期间(首次安装 npx 拉包)右下角显示初始化状态,完成即消失。 */
function BootstrapHint() {
  const [show, setShow] = useState(false);
  useEffect(() => {
    let disposed = false;
    let delayTimer = 0;
    let safetyTimer = 0;
    let unDone: (() => void) | undefined;
    getBootstrapState()
      .then((s) => {
        if (disposed || s !== 1) return;
        // 3 秒内完成的初始化不打扰(热缓存启动避免闪现);兜底 10 分钟自动消失
        delayTimer = window.setTimeout(() => {
          if (disposed) return;
          setShow(true);
          safetyTimer = window.setTimeout(() => setShow(false), 600_000);
        }, 3000);
      })
      .catch(() => {});
    void listen('bootstrap_done', () => {
      window.clearTimeout(delayTimer);
      window.clearTimeout(safetyTimer);
      setShow(false);
    }).then((f) => {
      unDone = f;
    });
    return () => {
      disposed = true;
      window.clearTimeout(delayTimer);
      window.clearTimeout(safetyTimer);
      unDone?.();
    };
  }, []);
  if (!show) return null;
  return (
    <div className="boot-hint" role="status">
      <span className="boot-spinner" aria-hidden="true" />
      正在初始化组件…
    </div>
  );
}

export default function App() {
  const refreshSessions = useSessionsStore((s) => s.refresh);
  const loadProviders = useProvidersStore((s) => s.load);
  const label = getCurrentWindow().label;

  useEffect(() => {
    // 托盘菜单窗口只需要主题与样式,不做会话/Provider 初始化
    if (label === 'tray-menu') return;
    // 每个窗口独立 document.title:避免任务栏/Alt-Tab 显示 dev server 地址(localhost:1420)
    // 读取用户自定义应用名称(空 = "myAgent")
    ipc.getSettings()
      .then((s) => {
        const name = s.custom_app_name?.trim() || 'myAgent';
        document.title = name;
        // document.title 不会同步到原生标题栏,需显式 setTitle(改名后即时生效)
        void getCurrentWindow().setTitle(name);
      })
      .catch(() => {
        document.title = 'myAgent';
      });
    // 监听设置变化,实时更新窗口标题
    const unTitle = listen('settings_changed', (e) => {
      const s = e.payload as ipc.AppSettings;
      const name = s.custom_app_name?.trim() || 'myAgent';
      document.title = name;
      void getCurrentWindow().setTitle(name);
    });

    // 主窗口启动时清掉遗留空会话(5 分钟宽限,避免误删刚创建还没发消息的会话)
    if (label === 'main') void useSessionsStore.getState().pruneEmpty(300);
    refreshSessions();
    // 启动即加载 Provider 模板与 Key 状态:漏掉这步会让全部模型选择器显示「未配置」
    void loadProviders();

    // H-1:保存/删除 Key、增删模板、拉取模型、改单模型配置后,
    // 后端广播 providers_changed,前端同步刷新全局 store。
    let unlisten: (() => void) | undefined;
    void listen('providers_changed', async () => {
      await useProvidersStore.getState().load();
      await useProvidersStore.getState().loadModelConfig();
    }).then((un) => {
      unlisten = un;
    });
    return () => {
      void unTitle.then((f) => f());
      unlisten?.();
    };
  }, [refreshSessions, loadProviders, label]);

  // 托盘右键菜单是独立的透明小窗:不进入主界面/引导门
  if (label === 'tray-menu') {
    return (
      <ErrorBoundary>
        <TrayMenu />
      </ErrorBoundary>
    );
  }

  return (
    <HashRouter>
      <DataDirGate />
      <OnboardingGate />
      {/* 启动提示只属于主窗口:小窗晚于 bootstrap 创建,收不到 bootstrap_done,会永久卡住 */}
      {label !== 'popout' && <BootstrapHint />}
      <ErrorBoundary>
        {label === 'popout' ? <PopoutApp /> : <MainApp />}
      </ErrorBoundary>
      <McpApprovalDialog />
      <ConfirmHost />
    </HashRouter>
  );
}
