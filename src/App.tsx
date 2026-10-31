import { HashRouter, Routes, Route, Navigate, useLocation, useNavigate, useParams } from 'react-router-dom';
import { lazy, Suspense, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import Titlebar from './components/Titlebar';
import Sidebar from './components/Sidebar';
import DataDirSetupDialog from './components/DataDirSetupDialog';
import OnboardingDialog from './components/OnboardingDialog';
import * as ipc from './lib/ipc';
import McpApprovalDialog from './components/McpApprovalDialog';
import { getBootstrapState } from './lib/ipc/mcp';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useSessionsStore } from './stores/sessions';
import { useProvidersStore } from './stores/providers';
import { useThemeStore } from './stores/theme';
import { resolveMode, useDefaultWorkbench } from './lib/workbench';
import './styles/base.css';
import './styles/dialog.css';
import './styles/home.css';
import './styles/chat.css';
import './styles/phase.css';
import './styles/tools.css';
import './styles/memory.css';
import './styles/settings.css';
import './styles/aiflavor.css';
import './styles/workbench.css';
import './styles/preset.css';
import './styles/study.css';
import 'katex/dist/katex.min.css';

// PERF-4:页面按路由懒加载,首屏只加载布局与当前页,避免 8 MB+ 单包一次性解析。
const HomePage = lazy(() => import('./pages/HomePage'));
const ChatPage = lazy(() => import('./pages/ChatPage'));
const WorkbenchHome = lazy(() => import('./pages/workbench/WorkbenchHome'));
const WorkbenchPage = lazy(() => import('./pages/workbench/WorkbenchPage'));
const StudyPage = lazy(() => import('./pages/StudyPage'));
// 整卷测试独立页面(学习中心内跳转)
const PaperPage = lazy(() =>
  import('./pages/study/PaperPage').then((m) => ({ default: m.PaperPage })),
);
const DrawPage = lazy(() => import('./pages/DrawPage'));
const TranslatePage = lazy(() => import('./pages/TranslatePage'));
const MemoryPage = lazy(() => import('./pages/MemoryPage'));
const NotesPage = lazy(() => import('./pages/NotesPage'));
const AttachmentsPage = lazy(() => import('./pages/AttachmentsPage'));
const AiFlavorPage = lazy(() => import('./pages/aiflavor/AiFlavorPage'));
const SettingsPage = lazy(() => import('./pages/SettingsPage'));

/** 学习规划/绘画/翻译/记忆/设置页自带布局,隐藏会话侧栏(条件挂载,抽屉 body class 随卸载清理)。 */
function MaybeSidebar() {
  const loc = useLocation();
  const bare = ['/plan', '/draw', '/translate', '/memory', '/notes', '/attachments', '/aiflavor', '/settings', '/work'];
  const hidden = bare.some((p) => loc.pathname.startsWith(p));
  if (hidden) return null;
  return <Sidebar />;
}

/** 首页门(D7):全局默认模式为 work 时落到专业首页 /work,否则普通首页。 */
function HomeGate() {
  const def = useDefaultWorkbench();
  if (def === null) return null;
  return def === 'work' ? <Navigate to="/work" replace /> : <HomePage />;
}

/** 会话模式门(D16):会话覆盖 > 全局默认;与目标路由不一致时重定向到对应形态。 */
function SessionModeGate({ mode }: { mode: 'chat' | 'work' }) {
  const { id } = useParams<{ id: string }>();
  const { search } = useLocation();
  const sessions = useSessionsStore((s) => s.sessions);
  const loaded = useSessionsStore((s) => s.loaded);
  const def = useDefaultWorkbench();
  if (!id) return <Navigate to="/" replace />;
  if (def === null) return null;
  const session = sessions.find((s) => s.id === id);
  // 会话不存在且列表已加载完成(如删除后从历史栈返回):回首页,避免落在空白会话页
  if (!session && loaded) return <Navigate to="/" replace />;
  if (session) {
    const eff = resolveMode(session.workbench_mode, def);
    if (eff !== mode) {
      // 保留 query(全局搜索/附件页的 ?msg=<id> 定位;模式重定向不能把它丢掉)
      const path = eff === 'work' ? `/work/${id}` : `/chat/${id}`;
      return <Navigate to={{ pathname: path, search }} replace />;
    }
  }
  return mode === 'work' ? (
    <WorkbenchPage>
      <ChatPage />
    </WorkbenchPage>
  ) : (
    <ChatPage />
  );
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

/** 主窗口内容:监听「跳转会话」/「打开设置页」/「划词提问」事件并导航。 */
function MainApp() {
  const nav = useNavigate();
  useEffect(() => {
    const unlistenSession = listen<{ sessionId: string }>('navigate-to-session', (e) => {
      // 先刷新会话列表:目标会话可能由其它窗口刚创建,否则模式门会因「本地不存在」而回首页
      void useSessionsStore
        .getState()
        .refresh()
        .catch(() => {})
        .finally(() => nav(`/chat/${e.payload.sessionId}`));
    });
    const unlistenSettings = listen<{ tab?: string }>('navigate-to-settings', (e) => {
      nav(e.payload.tab ? `/settings?tab=${e.payload.tab}` : '/settings');
    });
    // 划词提问:后端已唤起主窗口,这里新建会话并把选中文字预填进输入框
    const unlistenSelection = listen<{ text: string }>('selection_ask', (e) => {
      void (async () => {
        const text = e.payload.text.trim();
        if (!text) return;
        const current = useProvidersStore.getState().current;
        const title = `划词提问 ${new Date().toLocaleTimeString('zh-CN', {
          hour: '2-digit',
          minute: '2-digit',
        })}`;
        const { id } = await ipc.createSession(
          title,
          current ? `${current.providerId}/${current.model}` : '',
          null,
        );
        await useSessionsStore.getState().refresh();
        useSessionsStore.getState().setPendingComposer({ sessionId: id, text });
        nav(`/chat/${id}`);
      })().catch((err) => console.error('selection_ask failed', err));
    });
    // 外部编辑器改 memory.md frontmatter 的 conversation → 后端同步 sessions.title 后广播此事件
    const unlistenRenamed = listen('session_renamed', () => {
      void useSessionsStore.getState().refresh();
    });
    return () => {
      unlistenSession.then((f) => f());
      unlistenSettings.then((f) => f());
      unlistenSelection.then((f) => f());
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
        <MaybeSidebar />
        <div className="main">
          <Suspense fallback={null}>
            <Routes>
              <Route path="/" element={<HomeGate />} />
              <Route path="/chat/:id" element={<SessionModeGate mode="chat" />} />
              <Route path="/work" element={<WorkbenchHome />} />
              <Route path="/work/:id" element={<SessionModeGate mode="work" />} />
              <Route path="/plan" element={<StudyPage />} />
              <Route path="/plan/paper/:id" element={<PaperPage />} />
              <Route path="/draw" element={<DrawPage />} />
              <Route path="/translate" element={<TranslatePage />} />
              <Route path="/memory" element={<MemoryPage />} />
              <Route path="/notes" element={<NotesPage />} />
              <Route path="/attachments" element={<AttachmentsPage />} />
              <Route path="/aiflavor" element={<AiFlavorPage />} />
              <Route path="/settings" element={<SettingsPage />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
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

  return (
    <HashRouter>
      <DataDirGate />
      <OnboardingGate />
      <BootstrapHint />
      <ErrorBoundary>
        <MainApp />
      </ErrorBoundary>
      <McpApprovalDialog />
    </HashRouter>
  );
}
