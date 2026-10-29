/**
 * 去 AI 味 · AI 生成概率分析 —— 顶级工作台(R3 B3/B4,val.md §5)。
 *
 * 三栏:输入与历史 / 检测报告(仪表+雷达+热力图+规则命中) / 改写对照(流式)。
 * 检测走 Rust 本地统计(零 token);改写复用 chat_stream,由 ai-flavor-rewriter 承载。
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from '../../lib/ipc';
import {
  PILL_AGENTS,
  ANCHOR_KEY,
  stripImagePlaceholders,
  splitSentences,
  diffSentences,
  extractRewriteFull,
  extractPolished,
  extractAnchorCard,
} from './utils';
import { useGutterDrag } from './useGutterDrag';
import { useAiFlavorStream } from './useAiFlavorStream';
import { buildRewriteUserMsg, buildAgentMsg } from './aiflavorHelpers';
import { AiFlavorInputPanel } from './AiFlavorInputPanel';
import { AiFlavorReportPanel } from './AiFlavorReportPanel';
import { AiFlavorOutputPanel } from './AiFlavorOutputPanel';

export default function AiFlavorPage() {
  const [text, setText] = useState('');
  const [report, setReport] = useState<ipc.AiflavorReport | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [msg, setMsg] = useState('');
  const [history, setHistory] = useState<ipc.AiflavorReportSummary[]>([]);

  // 改写(R3 B4:chat_stream 复用)
  const [rewriteLevel, setRewriteLevel] = useState('light');
  const [rewriting, setRewriting] = useState(false);
  const [rewriteOut, setRewriteOut] = useState('');
  // 页头 pill = 智能体路由(val.md §5.5-4):四个专用智能体即默认入口
  const [activeAgent, setActiveAgent] = useState('ai-flavor-rewriter');
  // 非改写师智能体(检测官/学术合规/档案员)共用的右栏输出
  const [agentOut, setAgentOut] = useState('');
  const [agentRunning, setAgentRunning] = useState(false);
  // 身份锚开关:与聊天页同源(全裸模式不追加身份锚)
  const [bareMode, setBareMode] = useState(false);
  // 拖拽/选择文件载入(val.md §5.5-1):只提取文本,不落盘
  const [dragOver, setDragOver] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const imgFileRef = useRef<HTMLInputElement>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  /** 输入区附件图片(dataURL 数组)。本地检测跳过,AI agent 输出时多模态扫描。 */
  const [images, setImages] = useState<string[]>([]);
  // 命中规则点击定位到对应句子
  const [focusSent, setFocusSent] = useState<number | null>(null);
  // 风格锚点(深度档用)
  const [anchor, setAnchor] = useState(localStorage.getItem(ANCHOR_KEY) ?? '');
  const [showAnchor, setShowAnchor] = useState(false);
  // 右栏页签
  const [rightTab, setRightTab] = useState<'rewrite' | 'academic'>('rewrite');
  // C4:改写结果视图(逐句对照 / 原始输出)
  const [rewriteView, setRewriteView] = useState<'diff' | 'raw'>('diff');
  // 学术披露生成器三字段
  const [discModel, setDiscModel] = useState('');
  const [discUse, setDiscUse] = useState('辅助语言润色');

  // ============ 智能体流式调用 ============
  const { runCancel, startAgentStream } = useAiFlavorStream(bareMode, setMsg);

  // ============ 三栏 gutter 拖拽 ============
  // 全屏:gv1(左栏宽)、gv2(右栏宽);中屏:gv1+gh(下栏高);窄屏:gh1(左栏高)+gh2(右栏高)
  const gv1 = useGutterDrag('x', 'prev', '--aif-l-w', 'aif-l', 260, 180, 420);
  const gv2 = useGutterDrag('x', 'next', '--aif-r-w', 'aif-r', 320, 240, 480);
  const gh  = useGutterDrag('y', 'next', '--aif-b-h', 'aif-b', 200, 120, 480);
  const gh1 = useGutterDrag('y', 'prev', '--aif-l-h', 'aif-lh', 200, 100, 400);
  const gh2 = useGutterDrag('y', 'next', '--aif-r-h', 'aif-rh', 200, 100, 400);

  useEffect(() => {
    void refreshHistory();
    ipc.getSettings().then((s) => setBareMode(!!s.bare_mode)).catch(() => {});
  }, []);

  useEffect(() => {
    localStorage.setItem(ANCHOR_KEY, anchor);
  }, [anchor]);

  const refreshHistory = () =>
    ipc
      .aiflavorReportsList(50)
      .then(setHistory)
      .catch(() => {});

  const runAnalyze = async () => {
    const clean = stripImagePlaceholders(text);
    if (!clean) return;
    setAnalyzing(true);
    setMsg('');
    try {
      const r = await ipc.aiflavorAnalyze(clean);
      setReport(r);
      setFocusSent(null);
      void refreshHistory();
    } catch (e) {
      setMsg(`检测失败: ${e}`);
    } finally {
      setAnalyzing(false);
    }
  };

  const openHistory = async (id: number) => {
    try {
      setReport(await ipc.aiflavorReportGet(id));
      setMsg('');
    } catch (e) {
      setMsg(`读取报告失败: ${e}`);
    }
  };

  const removeHistory = async (id: number) => {
    try {
      await ipc.aiflavorReportDelete(id);
      if (report?.id === id) setReport(null);
      void refreshHistory();
    } catch (e) {
      setMsg(`删除失败: ${e}`);
    }
  };

  /** 拖入/选择文件:只提取文本并填入输入区(不落盘、不上传)。 */
  const loadFile = async (f: File) => {
    if (f.size <= 0) {
      setMsg(`「${f.name}」是空文件`);
      return;
    }
    if (f.size > ipc.MAX_ATTACHMENT_BYTES) {
      setMsg(`「${f.name}」超过 20MB 上限`);
      return;
    }
    setMsg(`正在提取「${f.name}」…`);
    try {
      const dataUrl = await ipc.readFileAsDataURL(f);
      const idx = dataUrl.indexOf(',');
      const b64 = idx >= 0 ? dataUrl.slice(idx + 1) : dataUrl;
      const t = await ipc.extractTextPreview(f.name, b64);
      setText(t);
      setReport(null);
      setMsg(`已载入「${f.name}」(${t.replace(/\s/g, '').length.toLocaleString()} 字)`);
    } catch (e) {
      setMsg(`读取文件失败: ${e}`);
    }
  };

  /** 从 FileList 里读图片 → dataURL → 追加到 images 数组,并在 textarea 光标位置插入占位符 `[图 N]`。 */
  const addImages = async (files: FileList | File[]) => {
    const imgs = Array.from(files).filter((f) => f.type.startsWith('image/'));
    if (imgs.length === 0) return;
    const newUrls: string[] = [];
    for (const f of imgs) {
      if (f.size > 8 * 1024 * 1024) {
        setMsg(`图片「${f.name}」超过 8MB,已跳过`);
        continue;
      }
      const url: string = await new Promise((resolve, reject) => {
        const r = new FileReader();
        r.onload = () => resolve(String(r.result));
        r.onerror = reject;
        r.readAsDataURL(f);
      });
      newUrls.push(url);
    }
    if (newUrls.length === 0) return;
    const baseIdx = images.length;
    const placeholders = newUrls.map((_, i) => `[图 ${baseIdx + i + 1}]`).join('\n');
    // 插入到 textarea 光标位置
    const ta = textRef.current;
    if (ta) {
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const before = text.slice(0, start);
      const after = text.slice(end);
      const sep = before && !before.endsWith('\n') ? '\n' : '';
      setText(`${before}${sep}${placeholders}\n${after}`);
      requestAnimationFrame(() => ta.focus());
    } else {
      setText((t) => `${t}${t && !t.endsWith('\n') ? '\n' : ''}${placeholders}\n`);
    }
    setImages((arr) => [...arr, ...newUrls]);
    setReport(null);
    setMsg(`已插入 ${newUrls.length} 张图片(本地检测自动跳过,AI agent 输出时会扫描)`);
  };

  /** 移除第 idx 张图(从 images 数组删 + 从 text 中删对应占位符)。 */
  const removeImage = (idx: number) => {
    setImages((arr) => arr.filter((_, i) => i !== idx));
    setText((t) => t.replace(new RegExp(`\\[图\\s*${idx + 1}\\]\\n?`, 'g'), ''));
  };
  /** 命中规则 → 定位到第一条相关句子(无单句可定位时给出说明)。 */
  const locateRule = (r: ipc.AiflavorRuleHit) => {
    if (!report) return;
    const needle = r.example.replace(/^[…\s]+|[…\s]+$/g, '').slice(0, 10);
    const marks = report.sentence_marks;
    const hit =
      marks.find((m) => needle && m.heat > 0 && m.text.includes(needle)) ??
      marks.find((m) => m.heat > 0);
    if (!hit) {
      setMsg(`「${r.name}」是全文统计特征，没有可定位的单句`);
      return;
    }
    setFocusSent(hit.index);
    requestAnimationFrame(() => {
      document
        .getElementById(`aif-sent-${hit.index}`)
        ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    });
  };

  const runRewrite = async () => {
    if (!report || rewriting) return;
    // 深度档按提示词约定必须有锚点:前置拦下,避免一次注定被模型追问的空调用
    if (rewriteLevel === 'deep' && !anchor.trim()) {
      setMsg('深度档需要风格锚点:请先填入旧文,或用「风格档案员」生成后再改写');
      return;
    }
    const userMsg = buildRewriteUserMsg(text, rewriteLevel, report, anchor);
    setActiveAgent('ai-flavor-rewriter');
    setRightTab('rewrite');
    setRewriting(true);
    setRewriteOut('');
    const acc = await startAgentStream('ai-flavor-rewriter', userMsg, setRewriteOut, images);
    setRewriting(false);
    if (acc === null || !report.id) return;
    // C4:保存改写结果时同时留档句级 diff(原文 ↔ 改写后全文)
    const full = extractRewriteFull(acc);
    const pairs = diffSentences(splitSentences(text), splitSentences(full));
    void ipc
      .aiflavorRewriteSave(
        report.id,
        rewriteLevel,
        JSON.stringify({ output: acc, extracted: full, pairs }),
        true,
      )
      .catch(() => {});
  };

  /** 运行当前 pill 对应的智能体(改写师委托 runRewrite 走 diff 视图)。 */
  const runAgent = async () => {
    if (agentRunning || rewriting) return;
    if (activeAgent === 'ai-flavor-rewriter') {
      await runRewrite();
      return;
    }
    const res = buildAgentMsg({
      agentId: activeAgent,
      report,
      text,
      anchor,
      discModel,
      discUse,
    });
    if (!res) return;
    if ('msg' in res) {
      setMsg(res.msg);
      return;
    }
    setRightTab('rewrite');
    setAgentRunning(true);
    setAgentOut('');
    await startAgentStream(activeAgent, res.userMsg, setAgentOut, images);
    setAgentRunning(false);
  };

  const stopRun = () => {
    runCancel.current?.();
    setRewriting(false);
    setAgentRunning(false);
  };

  /** 切回输入区:学术润色结果 / 风格锚点卡。 */
  const applyPolished = () => {
    const polished = extractPolished(agentOut);
    if (!polished) return;
    setText(polished);
    setAgentOut('');
    setReport(null);
    setMsg('润色文本已填回输入区，可再次检测对比分数变化');
  };

  const applyAnchor = () => {
    const card = extractAnchorCard(agentOut) || agentOut.trim();
    if (!card) return;
    setAnchor(card);
    setShowAnchor(true);
    setMsg('风格锚点卡已写入左侧锚点，深度档改写将以此为准');
  };

  /** 从改写输出提取「改写后全文」节,供应用回输入区;提取失败返回空。 */
  const extractedRewrite = useMemo(() => extractRewriteFull(rewriteOut), [rewriteOut]);

  const applyRewrite = () => {
    if (!extractedRewrite) return;
    setText(extractedRewrite);
    setReport(null);
    setRewriteOut('');
    setMsg('改写文本已填回输入区,可再次检测对比分数变化');
  };

  return (
    <div className="aif-page">
      <div className="aif-head">
        <div className="aif-head-main">

          <h2>去 AI 味 · AI 生成概率分析</h2>
          <p className="set-desc">
            面向你自己撰写的中文文本:检测哪里像 AI 写的、为什么,并给出一键人味化改写。
            结果是写作特征统计,不是抄袭或代写的证明。
          </p>
        </div>
        <div className="aif-pills" role="group" aria-label="去 AI 味智能体">
          {PILL_AGENTS.map((a) => (
            <button
              key={a.id}
              type="button"
              className={`aif-pill${activeAgent === a.id ? ' on' : ''}`}
              title={a.desc}
              onClick={() => {
                setActiveAgent(a.id);
                setRightTab('rewrite');
                setMsg('');
              }}
            >
              {a.label}
            </button>
          ))}
        </div>
      </div>

      {msg && <div className="aif-msg">{msg}</div>}

      <div className="aif-grid">
        {/* ============ 左栏:输入与历史 ============ */}
        <AiFlavorInputPanel
          text={text}
          setText={setText}
          analyzing={analyzing}
          onAnalyze={runAnalyze}
          images={images}
          dragOver={dragOver}
          setDragOver={setDragOver}
          onLoadFile={loadFile}
          onAddImages={addImages}
          onRemoveImage={removeImage}
          fileRef={fileRef}
          imgFileRef={imgFileRef}
          textRef={textRef}
          anchor={anchor}
          setAnchor={setAnchor}
          showAnchor={showAnchor}
          setShowAnchor={setShowAnchor}
          history={history}
          report={report}
          onOpenHistory={openHistory}
          onRemoveHistory={removeHistory}
        />

        {/* gutter 1:左栏-中栏 竖向分隔条 */}
        <div
          className={`aif-gutter aif-gutter-v gv1${gv1.dragging ? ' dragging' : ''}`}
          title="拖动调整左栏宽度"
          onPointerDown={gv1.onDown}
        />

        {/* ============ 中栏:检测报告 ============ */}
        <AiFlavorReportPanel
          report={report}
          focusSent={focusSent}
          onLocateRule={locateRule}
          rewriteLevel={rewriteLevel}
          onLevelChange={setRewriteLevel}
          anchor={anchor}
          rewriting={rewriting}
          onRewrite={runRewrite}
        />

        {/* gutter 2:中栏-右栏 竖向分隔条 */}
        <div
          className={`aif-gutter aif-gutter-v gv2${gv2.dragging ? ' dragging' : ''}`}
          title="拖动调整右栏宽度"
          onPointerDown={gv2.onDown}
        />

        {/* ============ 右栏:智能体输出 / 学术披露 ============ */}
        <AiFlavorOutputPanel
          rightTab={rightTab}
          setRightTab={setRightTab}
          activeAgent={activeAgent}
          text={text}
          rewriteOut={rewriteOut}
          rewriting={rewriting}
          rewriteView={rewriteView}
          setRewriteView={setRewriteView}
          extractedRewrite={extractedRewrite}
          report={report}
          agentOut={agentOut}
          agentRunning={agentRunning}
          discModel={discModel}
          setDiscModel={setDiscModel}
          discUse={discUse}
          setDiscUse={setDiscUse}
          onApplyRewrite={applyRewrite}
          onApplyPolished={applyPolished}
          onApplyAnchor={applyAnchor}
          onRunAgent={runAgent}
          onStop={stopRun}
          onMessage={setMsg}
        />

        {/* 横向 gutter:中屏下栏分隔 + 窄屏上下分隔(默认全部隐藏,CSS @media 控制显示) */}
        <div
          className={`aif-gutter aif-gutter-h gh${gh.dragging ? ' dragging' : ''}`}
          title="拖动调整下栏高度"
          onPointerDown={gh.onDown}
        />
        <div
          className={`aif-gutter aif-gutter-h gh1${gh1.dragging ? ' dragging' : ''}`}
          title="拖动调整左栏高度"
          onPointerDown={gh1.onDown}
        />
        <div
          className={`aif-gutter aif-gutter-h gh2${gh2.dragging ? ' dragging' : ''}`}
          title="拖动调整右栏高度"
          onPointerDown={gh2.onDown}
        />
      </div>
    </div>
  );
}
