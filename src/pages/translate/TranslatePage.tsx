import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import * as ipc from '../../lib/ipc';
import { useProvidersStore } from '../../stores/providers';
import { usePanelWidth } from '../../lib/usePanelWidth';
import TranslateForm, { langLabel } from './TranslateForm';
import GlossaryPanel from './GlossaryPanel';
import { useTranslateEngine } from './useTranslateEngine';

/** 对话页「翻译」按钮写入 sessionStorage 的 key。 */
const DRAFT_KEY = 'translate_draft';

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

  const { canRun, run, stop, skipNext } = useTranslateEngine({
    providerId,
    model,
    noModel,
    srcLang,
    dstLang,
    text,
    running,
    setResult,
    setRunning,
    setCopied,
    setError,
    setHistory,
  });

  const pickProvider = (id: string) => {
    setProviderId(id);
    const t = readyTemplates.find((x) => x.id === id);
    setModel(t?.models[0] ?? '');
  };

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
    skipNext(`${rec.source_text}\u0000${rec.source_lang ?? ''}\u0000${rec.target_lang}`);
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
      <GlossaryPanel
        readyTemplates={readyTemplates}
        providerId={providerId}
        model={model}
        noModel={noModel}
        activeTemplate={activeTemplate}
        onPickProvider={pickProvider}
        onModelChange={setModel}
        running={running}
        canRun={canRun}
        onRun={run}
        onStop={stop}
        srcChars={srcChars}
        text={text}
        glossary={glossary}
      />

      <TranslateForm
        readyCount={readyTemplates.length}
        srcLang={srcLang}
        dstLang={dstLang}
        onSrcLangChange={setSrcLang}
        onDstLangChange={setDstLang}
        onSwap={swap}
        text={text}
        onTextChange={setText}
        result={result}
        running={running}
        copied={copied}
        error={error}
        srcChars={srcChars}
        srcParas={srcParas}
        outChars={outChars}
        onCopySource={copySource}
        onCopyResult={copyResult}
        onToggleHistory={toggleHistory}
        onClearAll={clearAll}
        showHistory={showHistory}
        history={history}
        histPanel={histPanel}
        onApplyHistory={applyHistory}
        onRemoveHistory={removeHistory}
        pickOpen={pickOpen}
        onOpenPick={openPick}
        sessions={sessions}
        onClosePick={() => setPickOpen(false)}
        onSendToSession={sendToSession}
      />
    </div>
  );
}
