//! 产物页签(P6 / D14):会话附件 + 工具写出的工作区文件。
//! 布局对齐设计稿:分区(工作区文件 / 会话附件 / 产物预览)+ 居中文件卡 + 底部状态行。
//! 文件产物可「在应用中打开」(fs_open)与「定位目录」(fs_reveal);附件图片可内联预览。

import { useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { fsOpen, fsReveal, readAttachmentData } from '../../../../lib/ipc';
import { fmtBytes, type Artifact } from '../../../../lib/toolTrace';
import {
  IconDoc,
  IconDownload,
  IconExternalLink,
  IconInfo,
  IconRefresh,
} from '../../../../components/icons';
import type { WorkbenchData } from '../../useWorkbenchData';

function fmtTime(created: string): string {
  // created 形如 "2026-10-30 14:20:31" 或 ISO;取 HH:MM
  const m = /(\d{2}):(\d{2})/.exec(created);
  return m ? `${m[1]}:${m[2]}` : created;
}

function extTag(name: string): string {
  const i = name.lastIndexOf('.');
  return i >= 0 ? name.slice(i + 1).toUpperCase() : 'FILE';
}

function FileCard({ a, isAttachment }: { a: Artifact; isAttachment: boolean }) {
  const [, setSearchParams] = useSearchParams();
  const [preview, setPreview] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const jump = () => {
    if (a.messageId > 0) setSearchParams({ msg: String(a.messageId) });
  };
  const togglePreview = () => {
    if (preview) return setPreview(null);
    readAttachmentData(a.path)
      .then(setPreview)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  };

  const meta = isAttachment
    ? `用户附件${a.size != null ? ` · ${fmtBytes(a.size)}` : ''}`
    : `${a.step != null ? `由第 ${a.step} 步生成 · ` : ''}${fmtTime(a.created)}${a.size != null ? ` · ${fmtBytes(a.size)}` : ''}`;

  return (
    <div className="insp-filecard">
      <div className="insp-filecard-main">
        <span className="insp-filecard-ico">
          <IconDoc size={18} />
        </span>
        <button
          type="button"
          className="insp-filecard-name"
          title="跳到来源消息"
          onClick={jump}
        >
          {a.name}
        </button>
        <span className="insp-filecard-meta">{meta}</span>
        <span className="insp-filecard-ext">{extTag(a.name)}</span>
      </div>
      {error && <div className="insp-err">{error}</div>}
      {preview && (
        <button type="button" className="insp-filecard-previewwrap" onClick={togglePreview}>
          <img className="insp-artifact-preview" src={preview} alt={a.name} />
        </button>
      )}
      {!isAttachment && a.isImage && (
        <div className="insp-artifact-actions">
          <button type="button" className="btn-ghost btn-xs" onClick={togglePreview}>
            {preview ? '收起预览' : '预览图片'}
          </button>
        </div>
      )}
    </div>
  );
}

export function ArtifactsTab({ data }: { data: WorkbenchData }) {
  const [error, setError] = useState<string | null>(null);
  const atts = data.artifacts.filter((a) => a.kind === 'attachment');
  // 工作区文件:优先 traces(第 N 步生成);traces 缺失(旧数据 args 被截断)时回退到备份表推断
  let files = data.artifacts.filter((a) => a.kind === 'file');
  if (files.length === 0 && data.backups.length > 0) {
    const seen = new Map<string, (typeof files)[number]>();
    for (const b of data.backups) {
      if (seen.has(b.rel)) continue;
      const name = b.rel.split(/[\\/]/).filter(Boolean).pop() ?? b.rel;
      seen.set(b.rel, {
        kind: 'file',
        name,
        path: b.rel,
        size: b.bytes,
        messageId: 0,
        created: b.created,
        isImage: /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(name),
        step: null,
      });
    }
    files = [...seen.values()];
  }
  const previewFile = files[0] ?? null;

  if (files.length === 0 && atts.length === 0) {
    return <div className="insp-empty">还没有产物。模型写出文件或你发送附件后会列在这里。</div>;
  }

  return (
    <div className="insp-list">
      {/* 工作区文件 */}
      <div className="ins-label">
        工作区文件
        <span className="sp" />
        <button type="button" className="icon-btn" title="打开工作区目录" onClick={() => fsReveal('.').catch(() => {})}>
          <IconExternalLink size={13} />
        </button>
        <button
          type="button"
          className="icon-btn"
          title="刷新"
          onClick={() => data.refreshBackups().catch(() => {})}
        >
          <IconRefresh size={13} />
        </button>
        <span className="n">{files.length}</span>
      </div>
      {files.map((a) => (
        <FileCard key={`f:${a.messageId}:${a.path}`} a={a} isAttachment={false} />
      ))}
      {files.length === 0 && <div className="insp-empty">还没有工作区文件。</div>}

      {/* 会话附件 */}
      {atts.length > 0 && (
        <>
          <div className="ins-label">
            会话附件
            <span className="sp" />
            <span className="n">{atts.length}</span>
            <span className="insp-ro">· 只读</span>
          </div>
          {atts.map((a) => (
            <FileCard key={`a:${a.messageId}:${a.path}`} a={a} isAttachment />
          ))}
        </>
      )}

      {/* 产物预览(渲染最后一版工作区文件) */}
      {previewFile && (
        <>
          <div className="ins-label">
            产物预览
            <span className="sp" />
            <span className="n">第 1 / 1 页</span>
          </div>
          <div className="insp-preview-card">
            <div className="insp-preview-head">
              <span className="insp-filecard-ico">
                <IconDoc size={16} />
              </span>
              <span className="insp-preview-name">{previewFile.name}</span>
              <span className="sp" />
              <span className="insp-filecard-ext">{extTag(previewFile.name)}</span>
            </div>
            <div className="insp-preview-meta">
              {previewFile.step != null && `由第 ${previewFile.step} 步生成`}
              {previewFile.step != null && ' · '}
              {fmtTime(previewFile.created)}
              {previewFile.size != null && ` · ${fmtBytes(previewFile.size)}`}
            </div>
            <div className="insp-preview-actions">
              <button
                type="button"
                className="btn-primary btn-xs"
                onClick={() => fsOpen(previewFile.path).catch((e) => setError(String(e)))}
              >
                <IconExternalLink size={12} />
                在应用中打开
              </button>
              <button
                type="button"
                className="btn-ghost btn-xs"
                onClick={() => fsReveal(previewFile.path).catch((e) => setError(String(e)))}
              >
                定位目录
              </button>
              <span className="sp" />
              <button
                type="button"
                className="icon-btn"
                title="下载/导出"
                onClick={() => fsReveal(previewFile.path).catch((e) => setError(String(e)))}
              >
                <IconDownload size={14} />
              </button>
            </div>
          </div>
        </>
      )}

      {error && <div className="insp-err">{error}</div>}

      {/* 底部说明 + 状态行 */}
      <div className="insp-note">
        <IconInfo size={12} />
        只读附件不会写入工作区;产物预览仅渲染最后一版,历史版本在「变更与回滚」中查看。
      </div>
      <div className="insp-statusbar">
        产物 {files.length + atts.length}
        <span className="sp" />
        自动压缩 40 条
      </div>
    </div>
  );
}
