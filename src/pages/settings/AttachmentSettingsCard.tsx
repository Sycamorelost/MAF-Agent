//! 设置 · 存储位置 · 附件卡片:单文件夹文件数阈值 + 扩展名黑名单。

import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import {
  DEFAULT_BLOCKED_EXT,
  DEFAULT_FOLDER_LIMIT,
  DEFAULT_SESSION_MAX_FILES,
  DEFAULT_SESSION_MAX_MB,
  MAX_FOLDER_LIMIT,
  MAX_SESSION_MAX_FILES,
  MAX_SESSION_MAX_MB,
  clampFolderLimit,
  clampSessionMaxFiles,
  clampSessionMaxMb,
  normalizeExtList,
  parseExtInput,
} from '../../lib/attachmentPolicy';

export interface AttachmentSettingsCardProps {
  settings: ipc.AppSettings | null;
  /** 局部设置即时保存(失败会 throw) */
  onSave: (patch: Partial<ipc.AppSettings>) => Promise<void>;
}

export function AttachmentSettingsCard({ settings, onSave }: AttachmentSettingsCardProps) {
  const savedLimit = clampFolderLimit(settings?.attachment_folder_limit ?? DEFAULT_FOLDER_LIMIT);
  const savedExt = normalizeExtList(settings?.attachment_blocked_ext).join(', ');
  const savedMaxFiles = clampSessionMaxFiles(
    settings?.attachment_session_max_files ?? DEFAULT_SESSION_MAX_FILES,
  );
  const savedMaxMb = clampSessionMaxMb(settings?.attachment_session_max_mb ?? DEFAULT_SESSION_MAX_MB);
  const [limit, setLimit] = useState(String(savedLimit));
  const [ext, setExt] = useState(savedExt);
  const [maxFiles, setMaxFiles] = useState(String(savedMaxFiles));
  const [maxMb, setMaxMb] = useState(String(savedMaxMb));
  const [msg, setMsg] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // 设置被外部更新(首次加载 / 广播)时同步草稿
  useEffect(() => {
    setLimit(String(savedLimit));
    setExt(savedExt);
    setMaxFiles(String(savedMaxFiles));
    setMaxMb(String(savedMaxMb));
  }, [savedLimit, savedExt, savedMaxFiles, savedMaxMb]);

  const onSaveClick = async () => {
    setSaving(true);
    setMsg(null);
    try {
      const n = clampFolderLimit(Number(limit));
      const list = parseExtInput(ext);
      const files = clampSessionMaxFiles(Number(maxFiles));
      const mb = clampSessionMaxMb(Number(maxMb));
      await onSave({
        attachment_folder_limit: n,
        attachment_blocked_ext: list,
        attachment_session_max_files: files,
        attachment_session_max_mb: mb,
      });
      setLimit(String(n));
      setExt(list.join(', '));
      setMaxFiles(String(files));
      setMaxMb(String(mb));
      setMsg('已保存');
    } catch (e) {
      setMsg(`保存失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const failed = !!msg && msg.includes('失败');

  return (
    <div className="set-card">
      <div className="set-card-h">附件</div>
      <div className="set-card-b">
        <div className="set-row">
          <label className="lbl">单文件夹文件数阈值</label>
          <input
            className="pg-field"
            style={{ maxWidth: 'calc(96px * var(--ui-scale))' }}
            type="number"
            min={1}
            max={MAX_FOLDER_LIMIT}
            value={limit}
            onChange={(e) => setLimit(e.target.value)}
          />
          <span className="set-hint">个(1–{MAX_FOLDER_LIMIT})</span>
        </div>
        <p className="set-desc" style={{ margin: 'var(--space-2) 0 0' }}>
          拖入文件夹时生效:每个文件夹最多加入的文件数,超出部分自动跳过并提示。
          仍受单条消息 10 个附件的上限约束。
        </p>

        <div className="set-row" style={{ marginTop: 'var(--space-3)' }}>
          <label className="lbl">单会话数量配额</label>
          <input
            className="pg-field"
            style={{ maxWidth: 'calc(96px * var(--ui-scale))' }}
            type="number"
            min={1}
            max={MAX_SESSION_MAX_FILES}
            value={maxFiles}
            onChange={(e) => setMaxFiles(e.target.value)}
          />
          <span className="set-hint">个(1–{MAX_SESSION_MAX_FILES})</span>
          <input
            className="pg-field"
            style={{ maxWidth: 'calc(96px * var(--ui-scale))', marginLeft: 'var(--space-3)' }}
            type="number"
            min={1}
            max={MAX_SESSION_MAX_MB}
            value={maxMb}
            onChange={(e) => setMaxMb(e.target.value)}
          />
          <span className="set-hint">MB(1–{MAX_SESSION_MAX_MB})</span>
        </div>
        <p className="set-desc" style={{ margin: 'var(--space-2) 0 0' }}>
          单个会话的附件配额:同一文件被多次复用时只计一次。超限后新附件会被拒绝,
          并在附件中心的「拒绝记录」里留下原因。附件中心会按会话展示当前用量。
        </p>

        <div className="set-row" style={{ marginTop: 'var(--space-3)', alignItems: 'flex-start' }}>
          <label className="lbl">类型黑名单</label>
          <textarea
            className="pg-field"
            rows={3}
            value={ext}
            onChange={(e) => setExt(e.target.value)}
            style={{ resize: 'vertical' }}
          />
        </div>
        <p className="set-desc" style={{ margin: 'var(--space-2) 0 0' }}>
          这些扩展名的文件不允许加入会话(逗号或空格分隔,不区分大小写)。内置默认{' '}
          {DEFAULT_BLOCKED_EXT.length} 类始终生效,此处可在其基础上追加。
          除扩展名外,后端还会校验文件头,改后缀的可执行程序同样会被拒绝。
        </p>

        <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
          {msg && (
            <span className="set-hint" style={{ color: failed ? 'var(--err)' : 'var(--text-3)' }}>
              {msg}
            </span>
          )}
          <span className="sp" />
          <button className="btn-primary" onClick={onSaveClick} disabled={saving}>
            保存附件设置
          </button>
        </div>
      </div>
    </div>
  );
}
