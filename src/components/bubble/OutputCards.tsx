import { useEffect, useMemo, useState } from 'react';
import * as ipc from '../../lib/ipc';
import type { DeliverableDto } from '../../lib/ipc';
import { extractOutputFiles, outputBaseName } from '../../lib/outputFiles';
import { useDeliverablesStore } from '../../stores/deliverables';
import { IconCopy, IconDoc, IconExternalLink, IconFolder } from '../icons';

/** 卡片数据:后端产物事件优先,正文路径解析兜底时 note/kind 为空。 */
interface CardFile {
  key: string;
  path: string;
  name: string;
  note?: string;
  /** 展示用中文类型 */
  kind?: string;
  /** 后端原始类型(doc/sheet/slides/...),用于判断可执行的操作 */
  rawKind?: string;
  isImage: boolean;
}

const KIND_LABEL: Record<string, string> = {
  image: '图片',
  doc: '文档',
  sheet: '表格',
  slides: '演示文稿',
  pdf: 'PDF',
  archive: '压缩包',
  text: '文本',
  html: '网页',
  file: '文件',
};

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;

function fromDeliverable(d: DeliverableDto): CardFile {
  return {
    key: d.path.replace(/\\/g, '/').toLowerCase(),
    path: d.path,
    name: d.name || outputBaseName(d.path),
    note: d.note ?? undefined,
    kind: KIND_LABEL[d.kind] ?? '文件',
    rawKind: d.kind,
    isImage: d.kind === 'image' || IMAGE_EXT.test(d.path),
  };
}

/** 复制文本:优先异步剪贴板 API,不可用(部分 WebView / 非安全上下文)时退回 execCommand。 */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 降级到下面的兼容实现 */
  }
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

/** 办公文档才提供「转 PDF」(LibreOffice 无头转换)。 */
function canToPdf(file: CardFile) {
  return (
    ['doc', 'sheet', 'slides'].includes(file.rawKind ?? '') ||
    /\.(docx?|xlsx?|xls|csv|pptx?|ppt|odt|ods|odp|rtf)$/i.test(file.path)
  );
}

/** 操作:默认应用打开 / 文件管理器定位 / 复制路径 / 转 PDF。 */
function CardActions({ file, sessionId }: { file: CardFile; sessionId?: string }) {
  const [tip, setTip] = useState('');
  const [tipTitle, setTipTitle] = useState('');
  const [busy, setBusy] = useState(false);
  const path = file.path;

  /** 短暂显示操作结果;title 放完整路径/错误,避免卡片被撑开。 */
  const flash = (text: string, title = '') => {
    setTip(text);
    setTipTitle(title);
    window.setTimeout(() => {
      setTip('');
      setTipTitle('');
    }, 1800);
  };

  return (
    <div className="out-card-actions">
      <button
        type="button"
        className="out-act"
        title="用默认应用打开"
        onClick={() => void ipc.fsOpen(path, sessionId).catch(() => flash('打开失败'))}
      >
        <IconExternalLink size={13} />
      </button>
      <button
        type="button"
        className="out-act"
        title="在文件夹中显示"
        onClick={() => void ipc.fsReveal(path, sessionId).catch(() => flash('定位失败'))}
      >
        <IconFolder size={13} />
      </button>
      <button
        type="button"
        className="out-act"
        title="复制路径"
        onClick={() => void copyText(path).then((ok) => flash(ok ? '已复制' : '复制失败', path))}
      >
        <IconCopy size={13} />
      </button>
      {canToPdf(file) && (
        <button
          type="button"
          className="out-act out-act-pdf"
          title="转换为 PDF(需本机安装 LibreOffice)"
          aria-label="转换为 PDF"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            ipc
              .exportFilePdf(path, sessionId)
              .then((out) => flash('已生成', out))
              .catch((e: unknown) => flash('转换失败', String(e)))
              .finally(() => setBusy(false));
          }}
        >
          {busy ? '…' : 'PDF'}
        </button>
      )}
      {tip && (
        <span className="out-act-tip" title={tipTitle}>
          {tip}
        </span>
      )}
    </div>
  );
}

/** 图片产物:按需读取 data URL,点击放大。 */
function ImageCard({ file, sessionId }: { file: CardFile; sessionId?: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [zoom, setZoom] = useState(false);

  useEffect(() => {
    let alive = true;
    ipc
      .fsReadDataUrl(file.path, sessionId)
      .then((u) => alive && setUrl(u))
      .catch((e: unknown) => alive && setErr(String(e)));
    return () => {
      alive = false;
    };
  }, [file.path, sessionId]);

  return (
    <div className="out-card out-card-image">
      {url ? (
        <img className="out-thumb" src={url} alt={file.name} onClick={() => setZoom(true)} />
      ) : (
        <div className="out-thumb out-thumb-empty">{err ? '无法预览' : '加载图片…'}</div>
      )}
      <div className="out-card-foot">
        <span className="out-card-name" title={file.path}>
          {file.name}
        </span>
        <CardActions file={file} sessionId={sessionId} />
      </div>
      {zoom && url && (
        <div className="draw-lightbox" onClick={() => setZoom(false)}>
          <img src={url} alt={file.name} />
        </div>
      )}
    </div>
  );
}

/**
 * 会话产物卡片(M2):
 * 优先使用后端 `present` 工具登记的产物事件,正文路径解析仅作旧会话兜底,
 * 因此模型不写路径也能出卡。每张卡支持打开 / 定位 / 复制路径。
 */
export function OutputCards({
  content,
  sessionId,
  messageId,
}: {
  content: string;
  sessionId?: string;
  messageId?: number;
}) {
  const all = useDeliverablesStore((s) => (sessionId ? s.bySession[sessionId] : undefined));

  const files = useMemo(() => {
    const out: CardFile[] = [];
    const seen = new Set<string>();
    for (const d of all ?? []) {
      if (messageId == null || d.message_id !== messageId) continue;
      const c = fromDeliverable(d);
      if (seen.has(c.key)) continue;
      seen.add(c.key);
      out.push(c);
    }
    // 兜底:正文里明确写了路径、但后端没登记的旧会话
    for (const f of extractOutputFiles(content)) {
      const key = f.path.replace(/\\/g, '/').toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ key, path: f.path, name: f.name, isImage: f.isImage });
    }
    return out;
  }, [all, messageId, content]);

  if (files.length === 0) return null;
  return (
    <div className="out-cards">
      {files.map((f) =>
        f.isImage ? (
          <ImageCard key={f.key} file={f} sessionId={sessionId} />
        ) : (
          <div key={f.key} className="out-card">
            <IconDoc size={16} />
            <span className="out-card-info">
              <span className="out-card-name" title={f.path}>
                {f.name}
              </span>
              <span className="out-card-kind">
                {f.note ? `${f.kind ?? '文件'} · ${f.note}` : (f.kind ?? '产物')}
              </span>
            </span>
            <CardActions file={f} sessionId={sessionId} />
          </div>
        ),
      )}
    </div>
  );
}
