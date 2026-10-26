import { useEffect } from 'react';
import * as ipc from '../../lib/ipc';
import {
  IconImage,
  IconDownload,
  IconCopy,
  IconRefresh,
  IconUpload,
  IconCheck,
  IconChevronRight,
  IconChevronLeft,
} from '../../components/icons';

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** 当前展示的一张图(本次生成结果或历史条目放大)。 */
export interface ShownImage {
  path: string;
  dataUrl: string;
  meta?: ipc.ImageMeta | null;
}

interface Props {
  prompt: string;
  running: boolean;
  n: number;
  results: ShownImage[];
  history: ipc.ImageListItem[];
  histCollapsed: boolean;
  onToggleHist: () => void;
  onOpenHistory: (item: ipc.ImageListItem) => void;
  onPreview: (img: ShownImage) => void;
  preview: ShownImage | null;
  onClosePreview: () => void;
  onDownload: (img: ShownImage) => void;
  onCopy: (img: ShownImage) => void;
  onRepaint: (img: ShownImage) => void;
  onOpenUpload: (img: ShownImage) => void;
  uploadTarget: ShownImage | null;
  onCloseUpload: () => void;
  sessions: ipc.SessionRow[] | null;
  onUpload: (s: ipc.SessionRow) => void;
  toast: string | null;
}

/** 右侧画布:本次 2×2 结果 + 历史作品 + 放大预览 / 上传弹层 / toast。 */
export default function DrawResult({
  prompt,
  running,
  n,
  results,
  history,
  histCollapsed,
  onToggleHist,
  onOpenHistory,
  onPreview,
  preview,
  onClosePreview,
  onDownload,
  onCopy,
  onRepaint,
  onOpenUpload,
  uploadTarget,
  onCloseUpload,
  sessions,
  onUpload,
  toast,
}: Props) {
  /** Esc 关闭放大预览 / 上传弹层(与全局搜索一致,避免只能点遮罩) */
  useEffect(() => {
    if (!preview && !uploadTarget) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (uploadTarget) onCloseUpload();
      else onClosePreview();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preview, uploadTarget, onClosePreview, onCloseUpload]);

  return (
    <>
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
                    <div className="thumb" onClick={() => onPreview(img)} style={{ cursor: 'zoom-in' }}>
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
                        <button className="a" title="上传到对话" onClick={() => void onOpenUpload(img)}>
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
                onClick={onToggleHist}
              >
                {histCollapsed ? <IconChevronLeft size={13} /> : <IconChevronRight size={13} />}
              </button>
            </div>
            {!histCollapsed && (
              <div className="dw-hist-list">
                {history.length === 0 && <div className="rp-empty">暂无作品</div>}
                {history.map((item) => (
                  <div key={item.path} className="dw-hist-card" onClick={() => void onOpenHistory(item)}>
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
        <div className="draw-lightbox" onClick={onClosePreview}>
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
              <button className="btn-ghost" onClick={() => void onOpenUpload(preview)}>
                <IconUpload size={14} />
                上传到对话
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 上传到对话:会话选择 */}
      {uploadTarget && (
        <div className="dialog-mask" onClick={onCloseUpload}>
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
                  <button key={s.id} className="rp-sess" onClick={() => void onUpload(s)}>
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
    </>
  );
}

export { baseName };
