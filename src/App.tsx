import { HashRouter, Routes, Route, Navigate, useLocation, useNavigate } from 'react-router-dom';
import { lazy, Suspense, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import Titlebar from './components/Titlebar';
import Sidebar from './components/Sidebar';
import DataDirSetupDialog from './components/DataDirSetupDialog';
import * as ipc from './lib/ipc';
import McpApprovalDialog from './components/McpApprovalDialog';
import { useSessionsStore } from './stores/sessions';
import { useProvidersStore } from './stores/providers';
import { useThemeStore } from './stores/theme';
import './styles/app.css';
import 'katex/dist/katex.min.css';

// PERF-4:页面按路由懒加载,首屏只加载布局与当前页,避免 8 MB+ 单包一次性解析。
const HomePage = lazy(() => import('./pages/HomePage'));
const ChatPage = lazy(() => import('./pages/ChatPage'));
const ResearchPage = lazy(() => import('./pages/ResearchPage'));
const StudyPage = lazy(() => import('./pages/StudyPage'));
const DrawPage = lazy(() => import('./pages/DrawPage'));
const TranslatePage = lazy(() => import('./pages/TranslatePage'));
const MemoryPage = lazy(() => import('./pages/MemoryPage'));
const NotesPage = lazy(() => import('./pages/NotesPage'));
const AttachmentsPage = lazy(() => import('./pages/AttachmentsPage'));
const SettingsPage = lazy(() => import('./pages/SettingsPage'));

/** 深度研究/绘画/翻译/记忆/设置页自带布局,隐藏会话侧栏(条件挂载,抽屉 body class 随卸载清理)。 */
function MaybeSidebar() {
  const loc = useLocation();
  const bare = ['/research', '/plan', '/draw', '/translate', '/memory', '/notes', '/attachments', '/settings'];
  const hidden = bare.some((p) => loc.pathname.startsWith(p));
  if (hidden) return null;
  return <Sidebar />;
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

/** 主窗口内容:监听「跳转会话」/「打开设置页」/「划词提问」事件并导航。 */
function MainApp() {
  const nav = useNavigate();
  useEffect(() => {
    const unlistenSession = listen<{ sessionId: string }>('navigate-to-session', (e) => {
      nav(`/chat/${e.payload.sessionId}`);
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

  /** 启动时从 AppSettings 读取壁纸路径,读为 data URL 后挂载(WebView2 不支持 file://)。 */
  useEffect(() => {
    ipc
      .getSettings()
      .then(async (s) => {
        useThemeStore.getState().setWallpaperCover(s.wallpaper_cover ?? 0);
        if (s.wallpaper_path) {
          try {
            const dataUrl = await ipc.readWallpaper(s.wallpaper_path);
            useThemeStore.getState().setWallpaper(s.wallpaper_path, dataUrl);
          } catch (e) {
            console.error('[wallpaper] 读取壁纸失败:', e);
          }
        }
      })
      .catch((e) => console.error('[wallpaper] 读取设置失败:', e));
  }, []);

  return (
    <div className="window">
      <Titlebar />
      <div className="body">
        <MaybeSidebar />
        <div className="main">
          <Suspense fallback={null}>
            <Routes>
              <Route path="/" element={<HomePage />} />
              <Route path="/chat/:id" element={<ChatPage />} />
              <Route path="/research" element={<ResearchPage />} />
              <Route path="/plan" element={<StudyPage />} />
              <Route path="/draw" element={<DrawPage />} />
              <Route path="/translate" element={<TranslatePage />} />
              <Route path="/memory" element={<MemoryPage />} />
              <Route path="/notes" element={<NotesPage />} />
              <Route path="/attachments" element={<AttachmentsPage />} />
              <Route path="/settings" element={<SettingsPage />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </Suspense>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const refreshSessions = useSessionsStore((s) => s.refresh);
  const loadProviders = useProvidersStore((s) => s.load);
  const label = getCurrentWindow().label;

  useEffect(() => {
    // 每个窗口独立 document.title:避免任务栏/Alt-Tab 显示 dev server 地址(localhost:1420)
    document.title = 'MAF Agent';

    // 主窗口启动时清掉遗留空会话(5 分钟宽限,避免误删刚创建还没发消息的会话)
    if (label === 'main') void useSessionsStore.getState().pruneEmpty(300);
    refreshSessions();
    loadProviders();

    // H-1:保存/删除 Key、增删模板、拉取模型、改单模型配置后,
    // 后端广播 providers_changed,前端同步刷新全局 store。
    let unlisten: (() => void) | undefined;
    void listen('providers_changed', async () => {
      await useProvidersStore.getState().load();
      await useProvidersStore.getState().loadModelConfig();
    }).then((un) => {
      unlisten = un;
    });
    return () => unlisten?.();
  }, [refreshSessions, loadProviders, label]);

  return (
    <HashRouter>
      <DataDirGate />
      <MainApp />
      <McpApprovalDialog />
    </HashRouter>
  );
}
