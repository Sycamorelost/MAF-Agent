import * as ipc from '../../lib/ipc';
import { ResizeHandle } from '../../components/ResizeHandle';
import type { PanelResize } from '../../lib/usePanelWidth';
import {
  IconSwap,
  IconCopy,
  IconCheck,
  IconTrash,
  IconHistory,
  IconSend,
} from '../../components/icons';
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

export const langLabel = (v: string) => LANGS.find((l) => l.value === v)?.label ?? v;

interface Props {
  readyCount: number;
  srcLang: string;
  dstLang: string;
  onSrcLangChange: (v: string) => void;
  onDstLangChange: (v: string) => void;
  onSwap: () => void;
  text: string;
  onTextChange: (v: string) => void;
  result: string;
  running: boolean;
  copied: boolean;
  error: string | null;
  srcChars: number;
  srcParas: number;
  outChars: number;
  onCopySource: () => void;
  onCopyResult: () => void;
  onToggleHistory: () => void;
  onClearAll: () => void;
  showHistory: boolean;
  history: ipc.TranslationRecord[] | null;
  histPanel: PanelResize;
  onApplyHistory: (rec: ipc.TranslationRecord) => void;
  onRemoveHistory: (rec: ipc.TranslationRecord) => void;
  pickOpen: boolean;
  onOpenPick: () => void;
  sessions: ipc.SessionRow[] | null;
  onClosePick: () => void;
  onSendToSession: (s: ipc.SessionRow) => void;
}
/** 右侧翻译工作区:方向选择、原文/译文、历史侧栏与「发送到对话」弹层。 */
export default function TranslateForm({
  readyCount,
  srcLang,
  dstLang,
  onSrcLangChange,
  onDstLangChange,
  onSwap,
  text,
  onTextChange,
  result,
  running,
  copied,
  error,
  srcChars,
  srcParas,
  outChars,
  onCopySource,
  onCopyResult,
  onToggleHistory,
  onClearAll,
  showHistory,
  history,
  histPanel,
  onApplyHistory,
  onRemoveHistory,
  pickOpen,
  onOpenPick,
  sessions,
  onClosePick,
  onSendToSession,
}: Props) {
  return (
    <>
      {/* 右:翻译主区 */}
      <main className="pg-main">
        <div className="pg-main-head">
          {/* 语言方向选择器(从左栏移回主页):改动后自动重新翻译 */}
          <div className="tr-dir">
            <select
              className="pg-field"
              value={srcLang}
              onChange={(e) => onSrcLangChange(e.target.value)}
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
              onClick={onSwap}
            >
              <IconSwap size={15} />
            </button>
            <select
              className="pg-field"
              value={dstLang}
              onChange={(e) => onDstLangChange(e.target.value)}
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
          <button onClick={() => void onToggleHistory()} title="翻译历史">
            <IconHistory size={14} />
            历史
          </button>
          <button onClick={onClearAll} disabled={!text && !result && !error}>
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
                <button className="t" title="复制" disabled={!text} onClick={onCopySource}>
                  <IconCopy size={13} />
                </button>
                <button className="t" title="清空" disabled={!text} onClick={() => onTextChange('')}>
                  <IconTrash size={13} />
                </button>
              </div>
            </div>
            <textarea
              className="pane-body source"
              placeholder="粘贴文本 / 输入要翻译的内容…"
              value={text}
              onChange={(e) => onTextChange(e.target.value)}
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
                  onClick={() => void onOpenPick()}
                >
                  <IconSend size={13} />
                </button>
                <button className="t" title="复制译文" disabled={!result} onClick={onCopyResult}>
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
        {readyCount === 0 && (
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
                onClick={() => onApplyHistory(rec)}
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
                      void onRemoveHistory(rec);
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
        <div className="dialog-mask" onClick={onClosePick}>
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
                  <button key={s.id} className="rp-sess" onClick={() => onSendToSession(s)}>
                    <span className="ellipsis">{s.title || '未命名会话'}</span>
                    <IconCheck size={15} />
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
