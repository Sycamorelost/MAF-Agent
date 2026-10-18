//! 首次启动引导:选择数据存储位置。
//!
//! 未配置数据目录时(且非开发构建)由 App 挂载,用户必须选定目录后才能进入主界面。
//! 选定后写引导配置;若此前已有数据则下次启动自动迁移;需要重启时显示重启提示。

import { useEffect, useState } from 'react';
import * as ipc from '../lib/ipc';

export default function DataDirSetupDialog({ onDone }: { onDone: () => void }) {
  const [info, setInfo] = useState<ipc.DataDirInfo | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [restarting, setRestarting] = useState(false);

  useEffect(() => {
    ipc.getDataDir().then(setInfo).catch(() => {});
  }, []);

  const onPick = async () => {
    setMsg(null);
    try {
      const p = await ipc.pickDataDir();
      if (p) setPicked(p);
    } catch (e) {
      setMsg(`选择目录失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const onConfirm = async () => {
    if (!picked) return;
    setBusy(true);
    setMsg(null);
    try {
      const r = await ipc.setDataDir(picked, true);
      if (r.restart_required) {
        // 迁移在下一进程的 db 初始化之前执行,这里直接重启
        setRestarting(true);
        await ipc.restartApp();
      } else {
        onDone();
      }
    } catch (e) {
      setMsg(`设置失败:${e instanceof Error ? e.message : String(e)}`);
      setBusy(false);
    }
  };

  if (restarting) {
    return (
      <div className="dialog-mask">
        <div className="dialog">
          <div className="dialog-head">正在重启应用</div>
          <div className="dialog-body">
            <p className="set-hint">
              数据将迁移到你选择的目录,应用即将自动重新启动,请稍候…
            </p>
          </div>
        </div>
      </div>
    );
  }

  const same =
    picked !== null &&
    info !== null &&
    picked.replace(/[\\/]+$/, '').toLowerCase() ===
      info.path.replace(/[\\/]+$/, '').toLowerCase();

  return (
    <div className="dialog-mask">
      <div className="dialog">
        <div className="dialog-head">选择数据存储位置</div>
        <div className="dialog-body">
          <p className="set-hint" style={{ lineHeight: 1.7 }}>
            会话与消息、会话记忆、附件、加密密钥、生成图片等全部存放在你选择的目录。
            数据只保存在本机,不上传任何服务器。请选择一个空间充足、便于备份的目录。
          </p>

          {info && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              当前目录:
              <span style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-1)', wordBreak: 'break-all' }}>
                {' '}{info.path}
              </span>
            </div>
          )}

          {info && (info.legacy_adopted || info.has_data) && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              检测到已有数据位于当前目录,将自动迁移到你选择的目录(迁移完成后原目录会被删除)。
            </div>
          )}

          {info?.migration_error && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)', color: 'var(--err)' }}>
              上次迁移失败:{info.migration_error}
            </div>
          )}

          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-ghost" onClick={onPick} disabled={busy}>
              选择目录…
            </button>
            <button className="btn-primary" onClick={onConfirm} disabled={!picked || busy}>
              {same ? '完成' : '开始使用'}
            </button>
          </div>

          {picked && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              已选择:
              <span style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-1)', wordBreak: 'break-all' }}>
                {' '}{picked}
              </span>
              {same ? '(与当前目录相同,无需重启)' : '(应用将重启以完成迁移)'}
            </div>
          )}

          {msg && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)', color: 'var(--err)' }}>
              {msg}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
