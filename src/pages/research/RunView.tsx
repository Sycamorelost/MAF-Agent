import { useEffect, useMemo, useRef, useState } from 'react';
import { useResearchStore } from '../../stores/research';
import type { ActiveResearch } from '../../stores/research';
import { sanitizeHttpUrl } from '../../lib/url';
import { MemoMarkdown } from '../../components/MemoMarkdown';
import { ReportFigureImg } from './ReportFigureImg';
import { WriteMemoryDialog } from './WriteMemoryDialog';
import { StageTrail } from './StageTrail';
import * as ipc from '../../lib/ipc';
import { exportToPdf } from '../../lib/export/ExportService';
import { IconArrowLeft, IconBrain, IconChevronDown, IconDoc, IconDownload, IconGlobe, IconPause, IconPlay, IconStop } from '../../components/icons';
import { useCardResize } from '../../lib/usePanelWidth';
import { STATUS_LABEL } from './constants';
import { buildTimeline, activeCheckpoint, safeExportName, buildExportHtml, hostOf } from './helpers';

/** 导出格式(令牌写盘的三种文档格式 + 打印 PDF)。 */
type ExportKind = 'md' | 'html' | 'docx' | 'pdf';

/** 各格式的另存为对话框规格。 */
const EXPORT_SPEC = {
  md: { title: '导出研究报告(Markdown)', ext: 'md', filter: 'Markdown 文档' },
  html: { title: '导出研究报告(自包含 HTML)', ext: 'html', filter: 'HTML 文档' },
  docx: { title: '导出研究报告(Word)', ext: 'docx', filter: 'Word 文档' },
} as const;

export function RunView({ active }: { active: ActiveResearch }) {
  const { cancel, pause, resume, closeDetail } = useResearchStore();
  const cardRs = useCardResize();
  const tlRef = useRef<HTMLDivElement>(null);
  const mainRef = useRef<HTMLDivElement>(null);
  const reportRef = useRef<HTMLDivElement>(null);
  const [memDlg, setMemDlg] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportHint, setExportHint] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  // ---- 灰白分割(时间线 ↔ 报告比例):直接写 DOM 防拖拽重渲染,持久化 localStorage['rp-split'] ----
  /** 当前时间线高度(px);null = 默认比例(未自定义) */
  const splitRef = useRef<number | null>(null);
  const dragRef = useRef<{ startY: number; startH: number } | null>(null);

  const applySplit = (h: number) => {
    const tl = tlRef.current;
    const main = mainRef.current;
    if (!tl || !main) return;
    tl.style.flex = 'none';
    tl.style.height = `${h}px`;
    main.classList.add('split-custom');
  };

  // 挂载时恢复上次的分割位置
  useEffect(() => {
    const raw = Number(localStorage.getItem('rp-split'));
    if (Number.isFinite(raw) && raw >= 120) {
      splitRef.current = raw;
      applySplit(raw);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const resetSplit = () => {
    splitRef.current = null;
    localStorage.removeItem('rp-split');
    const tl = tlRef.current;
    if (tl) {
      tl.style.flex = '';
      tl.style.height = '';
    }
    mainRef.current?.classList.remove('split-custom');
  };

  const onSplitDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const tl = tlRef.current;
    if (!tl) return;
    dragRef.current = { startY: e.clientY, startH: tl.getBoundingClientRect().height };
    e.currentTarget.classList.add('dragging');
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onSplitMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const st = dragRef.current;
    const tl = tlRef.current;
    const main = mainRef.current;
    if (!st || !tl || !main) return;
    const max = Math.max(main.clientHeight - 140, 120); // 报告区至少保留 140px
    const h = Math.round(Math.max(120, Math.min(st.startH + (e.clientY - st.startY), max)));
    splitRef.current = h;
    applySplit(h);
  };

  const onSplitUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    e.currentTarget.classList.remove('dragging');
    e.currentTarget.releasePointerCapture(e.pointerId);
    if (splitRef.current) localStorage.setItem('rp-split', String(splitRef.current));
  };

  const nodes = useMemo(() => buildTimeline(active.trail, active.status), [active.trail, active.status]);
  const running = active.status === 'running';
  const paused = active.status === 'paused';
  const checkpoint = activeCheckpoint(active);
  const canExport = active.status === 'done' && !!active.report;

  // 报告图片:figures/ 引用走 IPC 加载(稳定引用,避免 MemoMarkdown 重渲染)
  const reportComponents = useMemo(
    () => ({
      img: ({ src, alt }: { src?: string; alt?: string }) => (
        <ReportFigureImg id={active.id} src={src ?? ''} alt={alt ?? ''} />
      ),
    }),
    [active.id],
  );

  // 贴底跟随:仅在用户处于底部附近时自动滚动,上翻阅读不被打断
  const stickBottom = useRef(true);
  const onTlScroll = () => {
    const el = tlRef.current;
    if (el) stickBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  };

  useEffect(() => {
    const el = tlRef.current;
    if (el && stickBottom.current) el.scrollTo({ top: el.scrollHeight });
  }, [nodes]);

  /** 通用导出流程:Markdown / HTML / Word 走令牌写盘;PDF 走原生打印对话框。 */
  const runExport = async (kind: ExportKind) => {
    setExportOpen(false);
    if (exporting) return;
    setExporting(true);
    setExportHint(null);
    try {
      if (kind === 'pdf') {
        if (!active.report.trim()) {
          setExportHint('报告为空,无法导出 PDF');
          return;
        }
        await exportToPdf(active.report, active.topic);
        setExportHint('已唤起打印对话框,请选择「另存为 PDF」');
        return;
      }
      const base = safeExportName(active.topic);
      const spec = EXPORT_SPEC[kind];
      const pick = await ipc.pickSavePath(
        spec.title,
        `${base}.${spec.ext}`,
        spec.filter,
        spec.ext,
      );
      if (!pick) return;
      const doExport = (overwrite: boolean) => {
        if (kind === 'docx') {
          // 研究报告归入「实践/调研报告」模板:宋体小四、1.5 倍行距、三线表。
          return ipc.researchExportDocx(active.id, pick.token, overwrite, 'practice');
        }
        if (kind === 'md') {
          return ipc.researchExportMarkdown(active.id, pick.token, overwrite);
        }
        const el = reportRef.current?.querySelector('.report-md');
        const html = buildExportHtml(active.topic, el?.innerHTML ?? '');
        return ipc.researchExportHtml(active.id, html, pick.token, overwrite);
      };
      let saved: string;
      try {
        saved = await doExport(pick.existed);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg === ipc.EXPORT_TARGET_EXISTS) {
          if (!window.confirm(`目标文件已存在:\n${pick.path}\n\n确认覆盖?`)) return;
          saved = await doExport(true);
        } else {
          throw e;
        }
      }
      setExportHint(saved ? `已导出:${saved}` : null);
    } catch (e) {
      setExportHint(`导出失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  const pct = active.maxRounds > 0 ? Math.min(100, (active.currentRound / active.maxRounds) * 100) : 0;
  const statusCls =
    active.status === 'done' ? 'ok' : active.status === 'error' ? 'err' : paused ? 'warn' : '';

  return (
    <div className="rp-run">
      <div className="rp-head">
        <button className="btn-ghost" onClick={closeDetail}>
          <IconArrowLeft size={14} />
          返回
        </button>
        <span className="topic" title={active.topic}>
          {active.topic}
        </span>
        <span
          className={`mode-tag ${
            active.mode === 'staged' ? 'staged' : active.mode === 'concise' ? 'concise' : 'legacy'
          }`}
        >
          {active.mode === 'staged' ? '专项' : active.mode === 'concise' ? '简洁' : '快速'}
        </span>
        <span className={`rp-status ${statusCls}`}>
          <i />
          {STATUS_LABEL[active.status] ?? active.status}
          {checkpoint
            ? ' · 检查点等待'
            : (running || paused) && ` · 第 ${active.currentRound} / ${active.maxRounds} 轮`}
        </span>
        {running && (
          <button className="btn-ghost" onClick={pause} title="在下一个检查点暂停">
            <IconPause size={14} />
            暂停
          </button>
        )}
        {paused && (
          <button className="btn-ghost" onClick={resume}>
            <IconPlay size={14} />
            继续
          </button>
        )}
        {(running || paused) && (
          <button className="btn-ghost" onClick={cancel}>
            <IconStop size={14} />
            终止
          </button>
        )}
        {active.status === 'done' && active.report && (
          <button className="btn-ghost" onClick={() => setMemDlg(true)} title="将报告摘要写入会话记忆">
            <IconBrain size={14} />
            写入记忆
          </button>
        )}
        {canExport && (
          <div className="rp-export-wrap">
            <button
              className="btn-ghost"
              onClick={() => setExportOpen((v) => !v)}
              disabled={exporting}
              title="导出研究报告"
            >
              <IconDownload size={14} />
              {exporting ? '导出中…' : '导出'}
              <IconChevronDown size={12} />
            </button>
            {exportOpen && (
              <>
                <div className="rp-export-mask" onClick={() => setExportOpen(false)} role="presentation" />
                <div className="rp-export-menu">
                  <button onClick={() => void runExport('md')}>
                    <IconDoc size={14} />
                    Markdown(.md)
                  </button>
                  <button onClick={() => void runExport('html')}>
                    <IconGlobe size={14} />
                    自包含 HTML(.html)
                  </button>
                  <button onClick={() => void runExport('docx')}>
                    <IconDoc size={14} />
                    Word 文档(.docx)
                  </button>
                  <button onClick={() => void runExport('pdf')}>
                    <IconDownload size={14} />
                    PDF(打印另存)
                  </button>
                  <div className="rp-export-note">
                    Word / HTML / PDF 统一学术版式(宋体小四、1.5 倍行距、三线表);
                    PDF 走系统打印对话框选「另存为 PDF」。
                  </div>
                </div>
              </>
            )}
          </div>
        )}
      </div>

      <div className="rp-progress">
        <i style={{ width: `${pct}%` }} />
      </div>

      {active.error && <div className="pg-warn">{active.error}</div>}

      <div className="rp-body">
        <div className="rp-main" ref={mainRef}>
          <div className="rp-timeline" ref={tlRef} onScroll={onTlScroll}>
            {active.mode === 'staged' && active.stages.length > 0 ? (
              <StageTrail active={active} checkpoint={checkpoint} />
            ) : (
              <>
                {nodes.length === 0 && <div className="rp-empty">正在启动研究…</div>}
                {nodes.map((n) => (
                  <div key={n.key} className={`rp-tl ${n.state}`}>
                    <div className="tt">
                      {n.title}
                      <span className={`pill${n.state === 'doing' ? ' warn' : ''}`}>
                        {n.state === 'doing' ? (paused ? '已暂停' : '运行中') : '完成'}
                      </span>
                    </div>
                    {n.queries.length > 0 && (
                      <div className="rp-queries">
                        {n.queries.map((q, i) => (
                          <span key={i}>{q}</span>
                        ))}
                      </div>
                    )}
                    {n.dd && (
                      <>
                        <div className="dd">{n.dd}</div>
                        <div
                          className="card-resize"
                          title="拖拽调整高度,双击复位"
                          onPointerDown={cardRs.onDown}
                          onPointerMove={cardRs.onMove}
                          onPointerUp={cardRs.onUp}
                          onDoubleClick={cardRs.onDoubleClick}
                        />
                      </>
                    )}
                  </div>
                ))}
              </>
            )}
          </div>

          {active.report && (
            <div
              className="rp-split"
              title="拖拽调整时间线与报告比例,双击复位"
              onPointerDown={onSplitDown}
              onPointerMove={onSplitMove}
              onPointerUp={onSplitUp}
              onDoubleClick={resetSplit}
            />
          )}

          {active.report && (
            <div className="rp-report" ref={reportRef}>
              <div className="hd">研究报告</div>
              {exportHint && <div className="rp-export-hint">{exportHint}</div>}
              <div className="md report-md">
                <MemoMarkdown
                  content={active.report + (running ? ' ▍' : '')}
                  withCodeBlock
                  streaming={running || paused}
                  extraComponents={reportComponents}
                />
              </div>
            </div>
          )}
        </div>

        <aside className="rp-sources">
          <div className="rp-src-head">
            已纳入来源 <span>{active.sources.length}</span>
          </div>
          <div className="rp-src-list">
            {active.sources.length === 0 && <div className="rp-empty">暂无来源</div>}
            {active.sources.map((s) => {
              const safeUrl = sanitizeHttpUrl(s.url);
              if (!safeUrl) {
                return (
                  <div key={s.id} className="rp-src" title="链接不安全或格式非法,已屏蔽">
                    <div className="t">{s.title}</div>
                    <div className="u">已屏蔽</div>
                    <div className="m">
                      <span>{s.engine}</span>
                      <span>[{s.id}]</span>
                    </div>
                  </div>
                );
              }
              return (
                <a key={s.id} className="rp-src" href={safeUrl} target="_blank" rel="noreferrer">
                  <div className="t">{s.title}</div>
                  <div className="u">{hostOf(safeUrl)}</div>
                  <div className="m">
                    <span>{s.engine}</span>
                    <span>[{s.id}]</span>
                  </div>
                </a>
              );
            })}
          </div>
        </aside>
      </div>

      {memDlg && <WriteMemoryDialog active={active} onClose={() => setMemDlg(false)} />}
    </div>
  );
}




