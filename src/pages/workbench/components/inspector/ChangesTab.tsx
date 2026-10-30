//! 变更与回滚页签(P6 / 对标设计稿):当前快照卡 + 文件变更列表 + Diff 卡 + 回滚粒度双卡。
//! 「仅恢复此文件」= 该文件最新一次备份;「恢复到此步」= 指定备份;恢复前均需确认。

import { useMemo, useState } from 'react';
import { fsBackupRead, fsReadText, fsRestore, type FsBackup } from '../../../../lib/ipc';
import { diffLines } from '../../../../lib/diffLines';
import { fmtBytes } from '../../../../lib/toolTrace';
import { IconClock, IconCompare, IconInfo, IconRefresh, IconRestore } from '../../../../components/icons';
import type { WorkbenchData } from '../../useWorkbenchData';

interface DiffState {
  rel: string;
  backupId: string;
  rows: ReturnType<typeof diffLines>;
}

function fmtTime(created: string): string {
  const m = /(\d{2}):(\d{2})/.exec(created);
  return m ? `${m[1]}:${m[2]}` : created;
}

export function ChangesTab({ data }: { data: WorkbenchData }) {
  const { backups, refreshBackups } = data;
  const [diff, setDiff] = useState<DiffState | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 当前选中要查看/回滚的备份(默认每文件最新一次)。 */
  const [selId, setSelId] = useState<string | null>(null);

  /** 按 rel 分组(组内保持时间倒序,后端 list 已按 created desc)。 */
  const groups = useMemo(() => {
    const map = new Map<string, FsBackup[]>();
    for (const b of backups) {
      const arr = map.get(b.rel) ?? [];
      arr.push(b);
      map.set(b.rel, arr);
    }
    return [...map.entries()];
  }, [backups]);

  /** 当前聚焦的备份(默认第一组第一个)。 */
  const focus: FsBackup | null =
    backups.find((b) => b.id === selId) ?? groups[0]?.[1][0] ?? null;

  const showDiff = async (b: FsBackup) => {
    setError(null);
    setSelId(b.id);
    try {
      const [oldText, newText] = await Promise.all([
        fsBackupRead(b.id),
        fsReadText(b.rel).catch(() => '(当前文件不存在或不可读)'),
      ]);
      setDiff({ rel: b.rel, backupId: b.id, rows: diffLines(oldText, newText) });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const restore = async (b: FsBackup, label: string) => {
    if (
      !window.confirm(
        `${label}\n\n文件:${b.rel}\n备份时间:${b.created}\n\n恢复前会自动再备份一次当前内容。`,
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await fsRestore(b.id);
      await refreshBackups();
      setDiff(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (groups.length === 0) {
    return <div className="insp-empty">还没有文件变更。模型写入/删除工作区文件前会自动备份。</div>;
  }

  const totalFiles = groups.length;
  const latest = backups[0];

  return (
    <div className="insp-list">
      {error && <div className="insp-err">{error}</div>}

      {/* 当前快照卡 */}
      {latest && (
        <div className="insp-snap">
          <div className="insp-snap-head">
            <span className="insp-snap-ico">
              <IconClock size={16} />
            </span>
            <div className="insp-snap-title">
              <div className="insp-snap-name">当前快照 · 第 {backups.length} 步</div>
              <div className="insp-snap-meta">
                {fmtTime(latest.created)} · {totalFiles} 个文件 · 可回滚
              </div>
            </div>
            <span className="insp-snap-tag">最新</span>
          </div>
        </div>
      )}

      {/* 文件变更列表 */}
      <div className="ins-label">
        文件变更
        <span className="sp" />
        <button
          type="button"
          className="icon-btn"
          title="刷新变更列表"
          onClick={() => void data.refreshBackups().catch(() => {})}
        >
          <IconRefresh size={13} />
        </button>
        <span className="n">{totalFiles}</span>
      </div>
      {groups.map(([rel, list]) => (
        <div key={rel} className="insp-change-row">
          <button
            type="button"
            className="insp-change-name"
            title="对比备份与当前文件"
            onClick={() => void showDiff(list[0])}
          >
            {rel}
          </button>
          <span className="insp-change-meta">
            {list.length} 次备份 · {fmtBytes(list[0].bytes)}
          </span>
          <button
            type="button"
            className="btn-ghost btn-xs"
            title="对比备份与当前文件"
            onClick={() => void showDiff(list[0])}
          >
            <IconCompare size={12} />
            对比
          </button>
        </div>
      ))}

      {/* Diff 卡 */}
      {diff && (
        <>
          <div className="ins-label">
            Diff
            <span className="sp" />
            <span className="n">{diff.rel}</span>
          </div>
          <div className="insp-diff">
            <div className="insp-diff-head">
              <span className="insp-diff-title">{diff.rel}</span>
              <span className="insp-diff-legend">
                <i className="del" /> 备份 <i className="add" /> 当前
              </span>
              <span className="sp" />
              <button type="button" className="btn-ghost btn-xs" onClick={() => setDiff(null)}>
                收起
              </button>
            </div>
            <div className="insp-diff-body">
              {diff.rows.map((r, i) => (
                <div
                  key={i}
                  className={`insp-diff-line${
                    r.left?.type === 'del' ? ' del' : r.right?.type === 'add' ? ' add' : ''
                  }`}
                >
                  {r.left?.text ?? r.right?.text ?? ''}
                </div>
              ))}
            </div>
          </div>
        </>
      )}

      {/* 回滚粒度双卡 */}
      <div className="ins-label">
        回滚粒度
        <span className="sp" />
        <span className="n">回滚会新建快照</span>
      </div>
      {focus && (
        <>
          <div className="insp-rollback">
            <div className="insp-rollback-title">仅恢复此文件</div>
            <div className="insp-rollback-desc">
              把 {focus.rel} 恢复到快照{fmtTime(focus.created)},其他文件保持当前版本。
            </div>
            <div className="insp-rollback-ops">
              <button
                type="button"
                className="btn-ghost btn-xs"
                disabled={busy}
                onClick={() => void restore(focus, '恢复此文件到该次备份?')}
              >
                <IconRestore size={12} />
                恢复此文件
              </button>
            </div>
          </div>
          <div className="insp-rollback">
            <div className="insp-rollback-title">恢复到此步</div>
            <div className="insp-rollback-desc">
              回滚该次备份涉及的文件并回到此步;对话记录与产物索引保留。
            </div>
            <div className="insp-rollback-ops">
              <button
                type="button"
                className="btn-brand"
                disabled={busy}
                onClick={() => void restore(focus, '恢复到此刻?该文件将回滚到此备份内容。')}
              >
                <IconRestore size={13} />
                恢复到此步
              </button>
            </div>
          </div>
        </>
      )}

      <div className="insp-note">
        <IconInfo size={12} />
        回滚不会删除历史,也不会撤回已发送的消息;PDF 为二进制文件,仅支持整文件替换。
      </div>
      <div className="insp-statusbar">
        快照 {backups.length}
        <span className="sp" />
        自动压缩 40 条
      </div>
    </div>
  );
}
