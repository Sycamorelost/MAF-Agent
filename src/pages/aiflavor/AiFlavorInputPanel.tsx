import type { Dispatch, RefObject, SetStateAction } from 'react';
import * as ipc from '../../lib/ipc';
import { CHAR_BUDGET, stripImagePlaceholders } from './utils';
import { IconChevronDown, IconChevronRight, IconX } from '../../components/icons';

/** 左栏:输入文本 / 风格锚点 / 历史报告。 */
export interface AiFlavorInputPanelProps {
  text: string;
  setText: Dispatch<SetStateAction<string>>;
  analyzing: boolean;
  onAnalyze: () => void;
  images: string[];
  dragOver: boolean;
  setDragOver: Dispatch<SetStateAction<boolean>>;
  onLoadFile: (f: File) => void;
  onAddImages: (files: FileList | File[]) => void;
  onRemoveImage: (idx: number) => void;
  fileRef: RefObject<HTMLInputElement | null>;
  imgFileRef: RefObject<HTMLInputElement | null>;
  textRef: RefObject<HTMLTextAreaElement | null>;
  anchor: string;
  setAnchor: Dispatch<SetStateAction<string>>;
  showAnchor: boolean;
  setShowAnchor: Dispatch<SetStateAction<boolean>>;
  history: ipc.AiflavorReportSummary[];
  report: ipc.AiflavorReport | null;
  onOpenHistory: (id: number) => void;
  onRemoveHistory: (id: number) => void;
}

export function AiFlavorInputPanel({
  text,
  setText,
  analyzing,
  onAnalyze,
  images,
  dragOver,
  setDragOver,
  onLoadFile,
  onAddImages,
  onRemoveImage,
  fileRef,
  imgFileRef,
  textRef,
  anchor,
  setAnchor,
  showAnchor,
  setShowAnchor,
  history,
  report,
  onOpenHistory,
  onRemoveHistory,
}: AiFlavorInputPanelProps) {
  const charCount = stripImagePlaceholders(text).replace(/\s/g, '').length;

  return (
    <section className="aif-col">
      <div
        className={`aif-card${dragOver ? ' drag-over' : ''}`}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          const files = e.dataTransfer.files;
          if (!files?.length) return;
          // 图片 → 加入图片数组;其他 → 提取文本
          const imgs = Array.from(files).filter((f) => f.type.startsWith('image/'));
          if (imgs.length > 0) onAddImages(imgs);
          const others = Array.from(files).filter((f) => !f.type.startsWith('image/'));
          if (others.length > 0) onLoadFile(others[0]);
        }}
      >
        <div className="aif-card-title">输入文本</div>
        <textarea
          ref={textRef}
          className="input aif-input"
          rows={10}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="粘贴或输入要检测的中文文本(论文、报告、申请文书、自媒体稿件…);也可以把 txt/md/docx/pdf/xlsx 拖进本卡片;图片也能拖入(本地检测自动跳过,AI agent 输出时会扫描)"
        />
        {/* 已插入的图片缩略图区 */}
        {images.length > 0 && (
          <div className="aif-img-strip">
            {images.map((url, i) => (
              <div key={i} className="aif-img-item" title={`图 ${i + 1}`}>
                <img src={url} alt={`图 ${i + 1}`} />
                <button
                  type="button"
                  className="aif-img-remove"
                  title="移除"
                  onClick={() => onRemoveImage(i)}
                >
                  <IconX size={14} />
                </button>
                <span className="aif-img-label">图 {i + 1}</span>
              </div>
            ))}
          </div>
        )}
        <input
          ref={fileRef}
          type="file"
          accept=".txt,.md,.markdown,.docx,.pdf,.xlsx,.xls,.csv,.json"
          style={{ display: 'none' }}
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) onLoadFile(f);
            e.target.value = '';
          }}
        />
        <input
          ref={imgFileRef}
          type="file"
          accept="image/*"
          multiple
          style={{ display: 'none' }}
          onChange={(e) => {
            const fs = e.target.files;
            if (fs?.length) onAddImages(fs);
            e.target.value = '';
          }}
        />
        <div className="aif-row">
          <span className={`aif-count${charCount > CHAR_BUDGET ? ' over' : ''}`}>
            {charCount.toLocaleString()} 字{images.length > 0 && ` · ${images.length} 张图`}
            {charCount > CHAR_BUDGET ? ' · 建议分段检测' : ''}
          </span>
          <button
            type="button"
            className="btn ghost"
            onClick={() => fileRef.current?.click()}
          >
            选择文件
          </button>
          <button
            type="button"
            className="btn ghost"
            onClick={() => imgFileRef.current?.click()}
          >
            插入图片
          </button>
          <button
            type="button"
            className="btn primary aif-cta"
            disabled={!stripImagePlaceholders(text) || analyzing}
            onClick={() => onAnalyze()}
          >
            {analyzing ? '检测中…' : '开始检测'}
          </button>
        </div>
      </div>

      <div className="aif-card">
        <button
          type="button"
          className="aif-collapse"
          onClick={() => setShowAnchor((v) => !v)}
        >
          风格锚点(可选) {showAnchor ? <IconChevronDown size={14} /> : <IconChevronRight size={14} />}
        </button>
        {showAnchor && (
          <>
            <textarea
              className="input aif-input"
              rows={5}
              value={anchor}
              onChange={(e) => setAnchor(e.target.value)}
              placeholder="粘贴 1-3 段你自己写过的旧文(邮件、笔记、报告均可)。改写师会用它提炼你的写作指纹。深度档改写必须有锚点。"
            />
            <div className="aif-hint">
              也可以在「风格档案员」智能体里让它输出结构化的风格锚点卡再粘贴回来。
            </div>
          </>
        )}
      </div>

      <div className="aif-card">
        <div className="aif-card-head">
          <div className="aif-card-title">历史报告</div>
          {history.length > 0 && <span className="aif-hint">{history.length} 条</span>}
        </div>
        {history.length === 0 && <div className="aif-hint">还没有检测记录</div>}
        <div className="aif-hist">
          {history.map((h) => (
            <div
              key={h.id}
              className={`aif-hist-item${report?.id === h.id ? ' on' : ''}`}
              onClick={() => onOpenHistory(h.id)}
            >
              <span className={`aif-badge lv${h.level === '低' ? 0 : h.level === '中' ? 1 : 2}`}>
                {h.score}
              </span>
              <span className="aif-hist-meta">
                {h.created?.slice(5, 16).replace('T', ' ')} · {h.char_count} 字 · {h.level}
              </span>
              <button
                type="button"
                className="btn ghost micro"
                title="删除该报告"
                onClick={(e) => {
                  e.stopPropagation();
                  onRemoveHistory(h.id);
                }}
              >
                删
              </button>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
