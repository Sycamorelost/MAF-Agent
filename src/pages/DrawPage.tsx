import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import * as ipc from '../lib/ipc';
import { useProvidersStore } from '../stores/providers';
import { TutorialList } from '../components/TutorialCard';
import { ResizeHandle } from '../components/ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
import {
  IconImage,
  IconSparkles,
  IconDownload,
  IconCopy,
  IconRefresh,
  IconUpload,
  IconCheck,
  IconChevronRight,
  IconChevronLeft,
} from '../components/icons';

/** 历史作品面板折叠状态缓存键 */
const HIST_COLLAPSED_KEY = 'draw-hist-collapsed';

const SIZES = [
  { value: '1024x1024', label: '1024×1024' },
  { value: '1792x1024', label: '1792×1024' },
  { value: '1024x1792', label: '1024×1792' },
];

/** 风格 chip:标签 + 合并进 prompt 的英文关键词。 */
const STYLES = [
  { id: 'none', label: '无', prompt: '' },
  { id: 'photo', label: '写实摄影', prompt: 'photorealistic, high detail, 8k' },
  { id: 'anime', label: '动漫', prompt: 'anime style, cel shading' },
  { id: 'watercolor', label: '水彩', prompt: 'watercolor painting, soft colors' },
  { id: 'oil', label: '油画', prompt: 'oil painting, textured brushstrokes' },
  { id: 'cyber', label: '赛博朋克', prompt: 'cyberpunk, neon lights, futuristic' },
  { id: '3d', label: '3D 渲染', prompt: '3d render, octane, studio lighting' },
  { id: 'sketch', label: '线稿', prompt: 'pencil sketch, monochrome line art' },
];

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** 当前展示的一张图(本次生成结果或历史条目放大)。 */
interface ShownImage {
  path: string;
  dataUrl: string;
  meta?: ipc.ImageMeta | null;
}

export default function DrawPage() {
  const { templates, hasKey, current } = useProvidersStore();
  const readyTemplates = templates.filter((t) => hasKey[t.id]);
  const nav = useNavigate();
  const asidePanel = usePanelWidth('draw-aside', 340, 260, 500);

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
      {/* 左:生成参数(宽度可拖拽,缓存本地) */}
      <aside className="pg-aside dw-params" style={{ width: asidePanel.width }} data-narrow={asidePanel.narrow}>
        <div className="pg-aside-head">
          <h2>
            <IconImage size={18} />
            生成参数
          </h2>
          <p>图片自动保存到 data/images/,参数留档同名 meta.json。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">生成模型</div>
          <select
            className="pg-field"
            value={providerId}
            onChange={(e) => pickProvider(e.target.value)}
            style={{ marginBottom: 'var(--space-2)' }}
          >
            {readyTemplates.length === 0 && <option value="">未配置</option>}
            {readyTemplates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.display_name}
              </option>
            ))}
          </select>
          <input
            className="pg-field"
            list="draw-image-models"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="如 dall-e-3 / cogview-3-plus"
          />
          <datalist id="draw-image-models">
            {(activeTemplate?.image_models ?? []).map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">Prompt</div>
          <textarea
            className="pg-area"
            rows={3}
            placeholder="描述想要的画面,例如:黄昏时分的湖边小屋,水彩风格"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
          />
          <textarea
            className="pg-area"
            rows={2}
            style={{ marginTop: 'var(--space-2)' }}
            placeholder="Negative prompt(可选):不希望出现的元素"
            value={negative}
            onChange={(e) => setNegative(e.target.value)}
          />
        </div>

        <div className="pg-sec">
          <div className="pg-lab">风格</div>
          <div className="pg-chips">
            {STYLES.map((s) => (
              <button
                key={s.id}
                className={`pg-chip${styleId === s.id ? ' on' : ''}`}
                onClick={() => setStyleId(s.id)}
              >
                {s.label}
              </button>
            ))}
          </div>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">输出</div>
          <div className="pg-chips">
            {SIZES.map((s) => (
              <button
                key={s.value}
                className={`pg-chip${size === s.value ? ' on' : ''}`}
                onClick={() => setSize(s.value)}
              >
                {s.label}
              </button>
            ))}
          </div>
          <div className="pg-row">
            <span>生成数量</span>
            <select className="pg-field num" value={n} onChange={(e) => setN(Number(e.target.value))}>
              {[1, 2, 3, 4].map((x) => (
                <option key={x} value={x}>
                  {x} 张
                </option>
              ))}
            </select>
          </div>
          <div className="pg-row">
            <span title="当前内置 Provider 不支持,已预留">种子 / Guidance</span>
            <span style={{ color: 'var(--text-4)', fontSize: 'var(--font-caption)' }}>
              预留(随 Provider 开放)
            </span>
          </div>
        </div>

        {activeTemplate && activeTemplate.image_models.length === 0 && (
          <div className="pg-warn">
            {activeTemplate.display_name} 未内置图像模型;若其支持 /images/generations
            可手填模型名,否则请换 OpenAI / 智谱。
          </div>
        )}
        {readyTemplates.length === 0 && (
          <div className="pg-warn">尚未配置任何 Provider 的 API Key,请先在对话页顶部模型菜单中配置。</div>
        )}
        {error && <div className="pg-warn">{error}</div>}

        <div className="pg-foot">
          <div className="pg-est">
            <span>出图规模</span>
            <b>
              {n} 张 · {SIZES.find((s) => s.value === size)?.label}
            </b>
          </div>
          <button className="pg-run" disabled={!canRun} onClick={run}>
            <IconSparkles size={15} />
            {running ? '生成中…' : '开始生成'}
          </button>
        </div>
        <TutorialList page="draw" />
      </aside>
      <ResizeHandle panel={asidePanel} />

      {/* 右:本次 2×2 画布 + 历史 */}
      <main className="dw-canvas">
        <div className="dw-head">
          <span className="topic">
            <IconImage size={16} />
            {prompt.trim() || '等待输入 Prompt'}
          </span>
          <span className="cost">
            本次 <b>{results.length}</b> 张 · 历史 <b>{history.length}</b> 张
          </span>
        </div>

        <div className="dw-body">
          <div className="dw-grid-wrap">
            <div className="dw-grid">
              {running &&
                Array.from({ length: n }).map((_, i) => (
                  <div key={`loading-${i}`} className="dw-card loading">
                    <div className="thumb">
                      <IconImage size={52} />
                      <span className="shim" />
                    </div>
                    <div className="meta">
                      <span className="info">生成中… {i + 1}/{n}</span>
                      <div className="acts" />
                    </div>
                  </div>
                ))}

              {!running && results.length === 0 && (
                <div className="dw-card empty">
                  <div className="thumb">
                    <IconImage size={52} />
                    <span>还没有生成结果</span>
                  </div>
                  <div className="meta">
                    <span className="info">待生成</span>
                    <div className="acts" />
                  </div>
                </div>
              )}

              {!running &&
                results.map((img) => (
                  <div key={img.path} className="dw-card">
                    <div className="thumb" onClick={() => setPreview(img)} style={{ cursor: 'zoom-in' }}>
                      <img src={img.dataUrl} alt="生成结果" />
                    </div>
                    <div className="meta">
                      <span className="info" title={img.path}>
                        {baseName(img.path)}
                      </span>
                      <div className="acts">
                        <button className="a" title="下载" onClick={() => onDownload(img)}>
                          <IconDownload size={13} />
                        </button>
                        <button className="a" title="复制" onClick={() => void onCopy(img)}>
                          <IconCopy size={13} />
                        </button>
                        <button className="a" title="以此为参考重绘" onClick={() => onRepaint(img)}>
                          <IconRefresh size={13} />
                        </button>
                        <button className="a" title="上传到对话" onClick={() => void openUploadDialog(img)}>
                          <IconUpload size={13} />
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
            </div>
          </div>

          <aside className={`dw-history${histCollapsed ? ' collapsed' : ''}`}>
            <div className="dw-hist-head">
              <span className="dw-hist-title">
                历史作品 <span className="count">{history.length} 张</span>
              </span>
              <button
                className="dw-hist-toggle"
                title={histCollapsed ? '展开历史作品' : '折叠历史作品'}
                onClick={toggleHist}
              >
                {histCollapsed ? <IconChevronLeft size={13} /> : <IconChevronRight size={13} />}
              </button>
            </div>
            {!histCollapsed && (
              <div className="dw-hist-list">
                {history.length === 0 && <div className="rp-empty">暂无作品</div>}
                {history.map((item) => (
                  <div key={item.path} className="dw-hist-card" onClick={() => void openHistory(item)}>
                    <div className="t">
                      <img src={item.thumb} alt="" />
                    </div>
                    <div className="d">
                      <span title={item.meta?.prompt ?? item.path}>
                        {item.meta?.prompt || baseName(item.path)}
                      </span>
                      <span>{item.meta ? `${item.meta.size}` : ''}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
            {histCollapsed && (
              <div className="dw-hist-collapsed-hint" title={`历史作品 ${history.length} 张`}>
                历史
              </div>
            )}
          </aside>
        </div>
      </main>

      {/* 放大预览 + 操作 */}
      {preview && (
        <div className="draw-lightbox" onClick={() => setPreview(null)}>
          <div className="draw-lightbox-inner" onClick={(e) => e.stopPropagation()}>
            <img src={preview.dataUrl} alt="放大预览" />
            {preview.meta && (
              <div className="draw-lightbox-meta">
                <div className="p">{preview.meta.prompt}</div>
                <div className="m">
                  {preview.meta.model} · {preview.meta.size}
                  {preview.meta.style ? ` · ${preview.meta.style}` : ''}
                </div>
              </div>
            )}
            <div className="draw-lightbox-acts">
              <button className="btn-ghost" onClick={() => onDownload(preview)}>
                <IconDownload size={14} />
                下载
              </button>
              <button className="btn-ghost" onClick={() => void onCopy(preview)}>
                <IconCopy size={14} />
                复制
              </button>
              <button className="btn-ghost" onClick={() => onRepaint(preview)}>
                <IconRefresh size={14} />
                重绘
              </button>
              <button className="btn-ghost" onClick={() => void openUploadDialog(preview)}>
                <IconUpload size={14} />
                上传到对话
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 上传到对话:会话选择 */}
      {uploadTarget && (
        <div className="dialog-mask" onClick={() => setUploadTarget(null)}>
          <div className="dialog" style={{ width: 440 }} onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">上传到对话</div>
            <div className="dialog-body">
              <p
                style={{
                  margin: '0 0 var(--space-3)',
                  color: 'var(--text-3)',
                  fontSize: 'var(--font-caption)',
                }}
              >
                图片将作为附件发送到所选会话。
              </p>
              {!sessions && <div className="rp-empty">加载会话列表…</div>}
              {sessions?.length === 0 && <div className="rp-empty">还没有会话,请先新建对话</div>}
              <div className="rp-sess-list">
                {sessions?.map((s) => (
                  <button key={s.id} className="rp-sess" onClick={() => void doUpload(s)}>
                    <span className="ellipsis">{s.title || '未命名会话'}</span>
                    <IconCheck size={15} />
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
