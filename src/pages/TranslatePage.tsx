import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import * as ipc from '../lib/ipc';
import { useProvidersStore } from '../stores/providers';
import { TutorialList } from '../components/TutorialCard';
import { ResizeHandle } from '../components/ResizeHandle';
import { usePanelWidth } from '../lib/usePanelWidth';
import AsideDrawer from '../components/AsideDrawer';
import {
  IconTranslate,
  IconSwap,
  IconCopy,
  IconCheck,
  IconTrash,
  IconStop,
  IconHistory,
  IconSend,
} from '../components/icons';

/** value 传给 LLM 的英文语言名;label 为界面显示。 */
const LANGS = [
  { value: 'Chinese', label: '中文' },
  { value: 'English', label: '英语' },
  { value: 'Japanese', label: '日语' },
  { value: 'Korean', label: '韩语' },
  { value: 'French', label: '法语' },
  { value: 'German', label: '德语' },
  { value: 'Russian', label: '俄语' },
  { value: 'Spanish', label: '西班牙语' },
];

/** 流式去抖:与消息流相同的 30ms 批量刷新策略。 */
const FLUSH_MS = 30;

/** 对话页「翻译」按钮写入 sessionStorage 的 key。 */
const DRAFT_KEY = 'translate_draft';

const langLabel = (v: string) => LANGS.find((l) => l.value === v)?.label ?? v;

/** token 粗估:与后端一致的 chars/3 口径。 */
const estTokens = (chars: number) => Math.ceil(chars / 3);

const fmtTok = (n: number) => (n >= 1000 ? `≈ ${(n / 1000).toFixed(1)}k tok` : `≈ ${n} tok`);

export default function TranslatePage() {
  const nav = useNavigate();
  const { templates, hasKey, current } = useProvidersStore();
  const readyTemplates = templates.filter((t) => hasKey[t.id]);
  const histPanel = usePanelWidth('translate-hist', 260, 200, 420, 'right');

  const [providerId, setProviderId] = useState('');
  const [model, setModel] = useState('');
  const [srcLang, setSrcLang] = useState(''); // '' = 自动检测
  const [dstLang, setDstLang] = useState('English');
  const [text, setText] = useState('');
  const [result, setResult] = useState('');
  const [running, setRunning] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // D6:术语表 / 历史 / 联动
  const [glossary, setGlossary] = useState<ipc.GlossaryEntry[]>([]);
  const [history, setHistory] = useState<ipc.TranslationRecord[] | null>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [pickOpen, setPickOpen] = useState(false);
  const [sessions, setSessions] = useState<ipc.SessionRow[] | null>(null);

  const bufRef = useRef('');
  /** 完整译文累积(去抖缓冲之外的全量,用于落历史) */
  const accRef = useRef('');
  const timerRef = useRef<number | undefined>(undefined);
  const cancelRef = useRef<(() => void) | null>(null);
  /** 跳过下一次自动翻译的签名(历史回填场景,避免覆盖已存译文) */
  const skipSig = useRef<string | null>(null);

  const activeTemplate = readyTemplates.find((t) => t.id === providerId);
  /** 无对话模型的专用翻译 Provider(DeepL):隐藏模型选择。 */
  const noModel = !!activeTemplate && activeTemplate.models.length === 0;

  // 默认沿用全局当前 Provider + 模型(跳过无模型的专用 Provider)
  useEffect(() => {
    if (!providerId && current) {
      const t = readyTemplates.find((x) => x.id === current.providerId);
      if (t) {
        setProviderId(current.providerId);
        setModel(t.models.includes(current.model) ? current.model : (t.models[0] ?? ''));
      }
    }
  }, [current, providerId, readyTemplates]);

  // 术语表加载(一次性;设置页改动后回到本页会重新挂载)
  useEffect(() => {
    ipc.glossaryList().then(setGlossary).catch(() => {});
  }, []);

  // 对话页「翻译」联动:捡起草稿
  useEffect(() => {
    try {
      const raw = sessionStorage.getItem(DRAFT_KEY);
      if (raw) {
        sessionStorage.removeItem(DRAFT_KEY);
        const draft = JSON.parse(raw) as { text?: string };
        if (draft.text) setText(draft.text);
      }
    } catch {
      // 忽略损坏草稿
    }
  }, []);

  // 卸载时取消流并清定时器
  useEffect(
    () => () => {
      cancelRef.current?.();
      if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const pickProvider = (id: string) => {
    setProviderId(id);
    const t = readyTemplates.find((x) => x.id === id);
    setModel(t?.models[0] ?? '');
  };

  /** 术语表命中统计(与后端匹配口径一致:子串 + 忽略大小写)。 */
  const glossaryHits = useMemo(() => {
    if (!text.trim() || glossary.length === 0) return [];
    const lower = text.toLowerCase();
    return glossary.filter((e) => {
      const s = e.src.trim();
      return s && (text.includes(s) || lower.includes(s.toLowerCase()));
    });
  }, [text, glossary]);

  const scheduleFlush = () => {
    if (timerRef.current !== undefined) return;
    timerRef.current = window.setTimeout(() => {
      timerRef.current = undefined;
      const pending = bufRef.current;
      bufRef.current = '';
      if (pending) setResult((r) => r + pending);
    }, FLUSH_MS);
  };

  /** 立即冲掉缓冲里未刷出的内容(完成/出错/停止时调)。 */
  const flushNow = () => {
    if (timerRef.current !== undefined) {
      window.clearTimeout(timerRef.current);
      timerRef.current = undefined;
    }
    const pending = bufRef.current;
    bufRef.current = '';
    if (pending) setResult((r) => r + pending);
  };

  const canRun = !!providerId && (noModel || !!model) && !!text.trim() && !running;

  const run = async () => {
    if (!providerId || (noModel ? false : !model) || !text.trim()) return;
    if (running) stop(); // 重新翻译:先停旧流(自动触发 / 手动重试共用)
    setError(null);
    setResult('');
    setCopied(false);
    bufRef.current = '';
    accRef.current = '';
    const srcText = text.trim();
    setRunning(true);

    const handle = ipc.translateStream(
      {
        provider_id: providerId,
        model: noModel ? providerId : model,
        text: srcText,
        source_lang: srcLang || null,
        target_lang: dstLang,
      },
      (chunk) => {
        if (chunk.type === 'delta') {
          accRef.current += chunk.content;
          bufRef.current += chunk.content;
          scheduleFlush();
        }
      },
    );
    cancelRef.current = handle.cancel;

    try {
      await handle.done;
      flushNow();
      // 落历史(仅完整完成;手动停止不存)
      const finalText = accRef.current.trim();
      if (finalText) {
        const rec: ipc.TranslationRecord = {
          created: new Date().toISOString(),
          provider_id: providerId,
          model: noModel ? '' : model,
          source_lang: srcLang || null,
          target_lang: dstLang,
          source_text: srcText,
          result_text: finalText,
        };
        ipc
          .saveTranslation(rec)
          .then((fileName) => {
            const saved = { ...rec, file_name: fileName };
            setHistory((h) => (h ? [saved, ...h] : h));
          })
          .catch(() => {});
      }
    } catch (e) {
      flushNow();
      setError(String(e));
    } finally {
      setRunning(false);
      cancelRef.current = null;
    }
  };

  const stop = () => {
    cancelRef.current?.();
    cancelRef.current = null;
    flushNow();
    setRunning(false);
  };

  // 自动翻译:输入原文 / 切换方向后 800ms 防抖触发,无需手动点击;
  // 清空文本立即停止并清空译文;历史回填(签名匹配)跳过一次,保留已存译文。
  useEffect(() => {
    if (skipSig.current !== null) {
      const sig = `${text}\u0000${srcLang}\u0000${dstLang}`;
      const match = skipSig.current === sig;
      skipSig.current = null;
      if (match) return;
    }
    if (!providerId || (!noModel && !model)) return;
    if (!text.trim()) {
      if (running) stop();
      setResult('');
      setError(null);
      return;
    }
    const t = window.setTimeout(() => void run(), 800);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, srcLang, dstLang, providerId, model]);

  const swap = () => {
    if (!srcLang) return; // 自动检测不可换到目标侧
    const a = srcLang;
    setSrcLang(dstLang);
    setDstLang(a);
    if (result) {
      setText(result);
      setResult('');
    }
  };

  const copyResult = async () => {
    if (!result) return;
    await navigator.clipboard.writeText(result);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const copySource = async () => {
    if (!text) return;
    await navigator.clipboard.writeText(text);
  };

  const clearAll = () => {
    if (running) stop();
    setText('');
    setResult('');
    setError(null);
  };

  // ---- 历史侧栏 ----

  const toggleHistory = async () => {
    const next = !showHistory;
    setShowHistory(next);
    if (next && history === null) {
      try {
        setHistory(await ipc.listTranslations());
      } catch {
        setHistory([]);
      }
    }
  };

  const applyHistory = (rec: ipc.TranslationRecord) => {
    if (running) stop();
    skipSig.current = `${rec.source_text}\u0000${rec.source_lang ?? ''}\u0000${rec.target_lang}`;
    setText(rec.source_text);
    setResult(rec.result_text);
    setSrcLang(rec.source_lang ?? '');
    setDstLang(rec.target_lang);
    setError(null);
  };

  const removeHistory = async (rec: ipc.TranslationRecord) => {
    if (!rec.file_name) return;
    try {
      await ipc.deleteTranslation(rec.file_name);
      setHistory((h) => h?.filter((x) => x.file_name !== rec.file_name) ?? h);
    } catch {
      // 静默
    }
  };

  // ---- 发送到对话 ----

  const openPick = async () => {
    setPickOpen(true);
    if (sessions === null) {
      try {
        setSessions(await ipc.listSessions());
      } catch {
        setSessions([]);
      }
    }
  };

  const sendToSession = async (s: ipc.SessionRow) => {
    const srcLabel = srcLang ? langLabel(srcLang) : '自动检测';
    const excerpt = text.length > 200 ? `${text.slice(0, 200)}…` : text;
    const content = `**[翻译 · ${srcLabel} → ${langLabel(dstLang)}]**\n\n${result}\n\n---\n\n> ${excerpt.replace(/\n/g, '\n> ')}`;
    try {
      await ipc.appendMessage({ session_id: s.id, role: 'user', content });
      setPickOpen(false);
      nav(`/chat/${s.id}`);
    } catch (e) {
      setError(String(e));
      setPickOpen(false);
    }
  };

  const srcChars = text.length;
  const srcParas = text.split(/\n+/).filter((s) => s.trim()).length;
  const outChars = result.length;

  return (
    <div className="pg">
      {/* 左:翻译设置(玻璃抽屉,标题栏「面板」按钮释放) */}
      <AsideDrawer>
      <aside className="pg-aside drawer">
        <div className="pg-aside-head">
          <h2>
            <IconTranslate size={18} />
            翻译设置
          </h2>
          <p>点标题栏「面板」可开合本栏;仅输出译文,保留原文格式与术语一致性。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">翻译引擎</div>
          <select
            className="pg-field"
            value={providerId}
            onChange={(e) => pickProvider(e.target.value)}
            style={{ marginBottom: noModel ? 0 : 'var(--space-2)' }}
          >
            {readyTemplates.length === 0 && <option value="">未配置</option>}
            {readyTemplates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.display_name}
              </option>
            ))}
          </select>
          {!noModel && (
            <select className="pg-field" value={model} onChange={(e) => setModel(e.target.value)}>
              {(activeTemplate?.models ?? []).map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
              {activeTemplate && !activeTemplate.models.includes(model) && model && (
                <option value={model}>{model}</option>
              )}
            </select>
          )}
          {noModel && <div className="pg-hint">专用翻译引擎,无需选择模型。</div>}
        </div>

        <div className="pg-sec">
          <div className="pg-lab">
            术语表
            <span className="sp" />
            <span className="n">{glossary.length} 条</span>
          </div>
          <div className="pg-hint">
            {glossary.length === 0
              ? '未配置术语表,可在 设置 → 翻译 中添加。'
              : glossaryHits.length > 0
                ? `原文命中 ${glossaryHits.length} 条,将按术语表翻译。`
                : '原文未命中任何词条。'}
          </div>
        </div>

        <div className="pg-foot">
          <div className="pg-est">
            <span>原文</span>
            <b>{srcChars} 字</b>
            <span className="pg-est-tok">{fmtTok(estTokens(srcChars))}</span>
          </div>
          {running ? (
            <button className="pg-run stop" onClick={stop}>
              <IconStop size={15} />
              停止
            </button>
          ) : (
            <button className="pg-run" disabled={!canRun} onClick={run}>
              <IconTranslate size={15} />
              开始翻译
            </button>
          )}
        </div>
        <TutorialList page="translate" />
      </aside>
      </AsideDrawer>

      {/* 右:翻译主区 */}
      <main className="pg-main">
        <div className="pg-main-head">
          {/* 语言方向选择器(从左栏移回主页):改动后自动重新翻译 */}
          <div className="tr-dir">
            <select
              className="pg-field"
              value={srcLang}
              onChange={(e) => setSrcLang(e.target.value)}
              title="源语言(自动检测 = 由模型识别)"
            >
              <option value="">自动检测</option>
              {LANGS.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
            <button
              className="pg-swap"
              title={srcLang ? '交换语言' : '自动检测时不可交换'}
              disabled={!srcLang}
              onClick={swap}
            >
              <IconSwap size={15} />
            </button>
            <select
              className="pg-field"
              value={dstLang}
              onChange={(e) => setDstLang(e.target.value)}
              title="目标语言"
            >
              {LANGS.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>
          <span className="sp" />
          <span className="cost">
            已输出 <b>{outChars}</b> 字
          </span>
          <button onClick={() => void toggleHistory()} title="翻译历史">
            <IconHistory size={14} />
            历史
          </button>
          <button onClick={clearAll} disabled={!text && !result && !error}>
            <IconTrash size={14} />
            清空
          </button>
        </div>

        <div className="tr-body">
          <div className="pane">
            <div className="pane-head">
              <span>原文</span>
              <span className="badge">{srcLang ? langLabel(srcLang) : '自动检测'}</span>
              <div className="tools">
                <button className="t" title="复制" disabled={!text} onClick={copySource}>
                  <IconCopy size={13} />
                </button>
                <button className="t" title="清空" disabled={!text} onClick={() => setText('')}>
                  <IconTrash size={13} />
                </button>
              </div>
            </div>
            <textarea
              className="pane-body source"
              placeholder="粘贴文本 / 输入要翻译的内容…"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <div className="pane-foot">
              <span>
                {srcChars} 字 · {srcParas} 段
              </span>
              <span>{srcLang ? '' : '源语言自动识别'}</span>
            </div>
          </div>

          <div className="pane">
            <div className="pane-head">
              <span>译文</span>
              {running && <span className="badge on">生成中</span>}
              <div className="tools">
                <button
                  className="t"
                  title="发送到对话"
                  disabled={!result || running}
                  onClick={() => void openPick()}
                >
                  <IconSend size={13} />
                </button>
                <button className="t" title="复制译文" disabled={!result} onClick={copyResult}>
                  {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
                </button>
              </div>
            </div>
            <div className={`pane-body target${result ? '' : ' empty'}`}>
              {result || '译文将显示在这里'}
              {running && <span className="cursor" />}
            </div>
            <div className="pane-foot">
              <span>{running ? '生成中…' : result ? '已完成' : '待翻译'}</span>
              <span>
                已输出 <b>{outChars}</b> 字
              </span>
            </div>
          </div>
        </div>

        {readyTemplates.length === 0 && (
          <div className="pg-warn" style={{ margin: '0 var(--pad-x) var(--pad-x)' }}>
            尚未配置任何 Provider 的 API Key,请先在对话页顶部模型菜单中配置。
          </div>
        )}
        {error && (
          <div className="pg-warn" style={{ margin: '0 var(--pad-x) var(--pad-x)' }}>
            {error}
          </div>
        )}
      </main>

      {/* 右:历史侧栏(宽度可拖拽,缓存本地) */}
      {showHistory && (
        <>
          <ResizeHandle panel={histPanel} />
          <aside
            className="pg-aside right"
            style={{ width: histPanel.width }}
            data-narrow={histPanel.narrow}
          >
          <div className="pg-aside-head">
            <h2>
              <IconHistory size={18} />
              翻译历史
            </h2>
            <p>最近 100 条,点击回填到编辑区。</p>
          </div>
          <div className="tr-hist-list">
            {history === null && <div className="rp-empty">加载中…</div>}
            {history?.length === 0 && <div className="rp-empty">暂无翻译记录</div>}
            {history?.map((rec) => (
              <div
                key={rec.file_name || rec.created}
                className="tr-hist-item"
                onClick={() => applyHistory(rec)}
              >
                <div className="h-top">
                  <span className="h-lang">
                    {rec.source_lang ? langLabel(rec.source_lang) : '自动'} →{' '}
                    {langLabel(rec.target_lang)}
                  </span>
                  <span className="h-time">
                    {new Date(rec.created).toLocaleString('zh-CN', {
                      month: '2-digit',
                      day: '2-digit',
                      hour: '2-digit',
                      minute: '2-digit',
                    })}
                  </span>
                  <button
                    className="h-del"
                    title="删除记录"
                    onClick={(e) => {
                      e.stopPropagation();
                      void removeHistory(rec);
                    }}
                  >
                    <IconTrash size={12} />
                  </button>
                </div>
                <div className="h-src ellipsis">{rec.source_text}</div>
                <div className="h-dst ellipsis">{rec.result_text}</div>
              </div>
            ))}
          </div>
        </aside>
        </>
      )}

      {/* 发送到对话:会话选择 */}
      {pickOpen && (
        <div className="dialog-mask" onClick={() => setPickOpen(false)}>
          <div className="dialog" style={{ width: 440 }} onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">发送到对话</div>
            <div className="dialog-body">
              <p
                style={{
                  margin: '0 0 var(--space-3)',
                  color: 'var(--text-3)',
                  fontSize: 'var(--font-caption)',
                }}
              >
                译文将连同原文摘要发送到所选会话。
              </p>
              {!sessions && <div className="rp-empty">加载会话列表…</div>}
              {sessions?.length === 0 && <div className="rp-empty">还没有会话,请先新建对话</div>}
              <div className="rp-sess-list">
                {sessions?.map((s) => (
                  <button key={s.id} className="rp-sess" onClick={() => void sendToSession(s)}>
                    <span className="ellipsis">{s.title || '未命名会话'}</span>
                    <IconCheck size={15} />
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
