import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import * as ipc from '../../lib/ipc';
import { useProvidersStore } from '../../stores/providers';
import DrawForm, { STYLES } from './DrawForm';
import DrawResult, { baseName, type ShownImage } from './DrawResult';

/** 历史作品面板折叠状态缓存键 */
const HIST_COLLAPSED_KEY = 'draw-hist-collapsed';

export default function DrawPage() {
  const { templates, hasKey, current } = useProvidersStore();
  const readyTemplates = templates.filter((t) => hasKey[t.id]);
  const nav = useNavigate();

  const [providerId, setProviderId] = useState('');
  const [model, setModel] = useState('');
  const [prompt, setPrompt] = useState('');
  const [negative, setNegative] = useState('');
  const [styleId, setStyleId] = useState('none');
  const [size, setSize] = useState('1024x1024');
  const [n, setN] = useState(1);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 本次会话生成结果(含完整 data_url) */
  const [results, setResults] = useState<ShownImage[]>([]);
  /** 历史作品(缩略图) */
  const [history, setHistory] = useState<ipc.ImageListItem[]>([]);
  const [preview, setPreview] = useState<ShownImage | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  /** 「上传到对话」目标选择 */
  const [uploadTarget, setUploadTarget] = useState<ShownImage | null>(null);
  const [sessions, setSessions] = useState<ipc.SessionRow[] | null>(null);
  /** 历史面板折叠状态 */
  const [histCollapsed, setHistCollapsed] = useState(() => {
    try { return localStorage.getItem(HIST_COLLAPSED_KEY) === '1'; } catch { return false; }
  });

  const activeTemplate = readyTemplates.find((t) => t.id === providerId);
  const activeStyle = STYLES.find((s) => s.id === styleId) ?? STYLES[0];

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2200);
  };

  const loadHistory = useCallback(async () => {
    try {
      setHistory(await ipc.listImages());
    } catch {
      /* 历史读取失败不阻塞主流程 */
    }
  }, []);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  /** 切换 Provider 时自动匹配其图像模型;无内置图像模型则清空待手填。 */
  const pickProvider = (id: string) => {
    setProviderId(id);
    const t = readyTemplates.find((x) => x.id === id);
    setModel(t?.image_models[0] ?? '');
  };

  // 默认沿用全局当前选中的 Provider(已配 Key)
  useEffect(() => {
    if (!providerId && current) pickProvider(current.providerId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, providerId]);

  const canRun = !!providerId && !!model.trim() && !!prompt.trim() && !running;

  const run = async () => {
    if (!canRun) return;
    setRunning(true);
    setError(null);
    try {
      const customTpl =
        providerId === 'custom' ? templates.find((t) => t.id === 'custom') : null;
      const list = await ipc.generateImage({
        provider_id: providerId,
        model: model.trim(),
        prompt: prompt.trim(),
        negative: negative.trim() || null,
        style_prompt: activeStyle.prompt || null,
        style: activeStyle.id === 'none' ? null : activeStyle.label,
        size,
        n,
        custom_base_url: customTpl?.base_url ?? null,
        custom_display_name: customTpl?.display_name ?? null,
      });
      const shown = list.map((r) => ({ path: r.path, dataUrl: r.data_url }));
      setResults(shown);
      void loadHistory();
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  /* ---------- 操作条 ---------- */

  const onDownload = (img: ShownImage) => {
    const a = document.createElement('a');
    a.href = img.dataUrl;
    a.download = baseName(img.path);
    a.click();
  };

  const onCopy = async (img: ShownImage) => {
    try {
      const blob = await (await fetch(img.dataUrl)).blob();
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
      showToast('已复制到剪贴板');
    } catch {
      showToast('复制失败');
    }
  };

  /** 图生图(当前 Provider 无 img2img):回填该图参数,改词重绘。 */
  const onRepaint = (img: ShownImage) => {
    const m = img.meta;
    if (!m) {
      showToast('该图无参数留档,无法回填');
      return;
    }
    setPrompt(m.prompt);
    setNegative(m.negative ?? '');
    setStyleId(STYLES.find((s) => s.label === m.style)?.id ?? 'none');
    setSize(m.size);
    if (readyTemplates.some((t) => t.id === m.provider_id)) {
      setProviderId(m.provider_id);
      setModel(m.model);
    }
    showToast('参数已回填,可修改后重新生成');
  };

  const openUploadDialog = async (img: ShownImage) => {
    setUploadTarget(img);
    setSessions(null);
    try {
      setSessions(await ipc.listSessions());
    } catch (e) {
      showToast(String(e));
      setUploadTarget(null);
    }
  };

  const doUpload = async (s: ipc.SessionRow) => {
    const img = uploadTarget;
    if (!img) return;
    try {
      const meta = await ipc.imageToSession(img.path, s.id);
      const label = img.meta?.prompt ? `「${img.meta.prompt}」` : '';
      await ipc.appendMessage({
        session_id: s.id,
        role: 'user',
        content: `[来自 AI 绘画] ${label}`.trim(),
        model_id: null,
        tokens: 0,
        attachments: JSON.stringify([meta]),
        sources: null,
      });
      setUploadTarget(null);
      nav(`/chat/${s.id}`);
    } catch (e) {
      showToast(String(e));
    }
  };

  /** 切换历史面板折叠状态并持久化 */
  const toggleHist = () => {
    setHistCollapsed((c) => {
      const next = !c;
      try { localStorage.setItem(HIST_COLLAPSED_KEY, next ? '1' : '0'); } catch { /* ignore */ }
      return next;
    });
  };

  /** 历史条目点击:读全量图放大预览。 */
  const openHistory = async (item: ipc.ImageListItem) => {
    try {
      const dataUrl = await ipc.readImage(item.path);
      setPreview({ path: item.path, dataUrl, meta: item.meta });
    } catch (e) {
      showToast(String(e));
    }
  };

  return (
    <div className="pg">
      {/* 左:生成参数(玻璃抽屉,标题栏「面板」按钮释放) */}
      <DrawForm
        readyTemplates={readyTemplates}
        providerId={providerId}
        model={model}
        activeTemplate={activeTemplate}
        onPickProvider={pickProvider}
        onModelChange={setModel}
        prompt={prompt}
        onPromptChange={setPrompt}
        negative={negative}
        onNegativeChange={setNegative}
        styleId={styleId}
        onStyleChange={setStyleId}
        size={size}
        onSizeChange={setSize}
        n={n}
        onNChange={setN}
        error={error}
        running={running}
        canRun={canRun}
        onRun={run}
      />

      <DrawResult
        prompt={prompt}
        running={running}
        n={n}
        results={results}
        history={history}
        histCollapsed={histCollapsed}
        onToggleHist={toggleHist}
        onOpenHistory={openHistory}
        onPreview={setPreview}
        preview={preview}
        onClosePreview={() => setPreview(null)}
        onDownload={onDownload}
        onCopy={onCopy}
        onRepaint={onRepaint}
        onOpenUpload={openUploadDialog}
        uploadTarget={uploadTarget}
        onCloseUpload={() => setUploadTarget(null)}
        sessions={sessions}
        onUpload={doUpload}
        toast={toast}
      />
    </div>
  );
}
