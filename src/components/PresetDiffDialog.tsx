//! 预设切换差异确认弹窗(P5 / D19):五维差异表(工具/上下文/记忆/审批/检索),
//! 显式确认后才钉住新档位;审批差异仅为预设默认值,实际仍以会话权限档为准。
//! 放在通用 components(见 val.md §5.8 文件拆分)。

import type { Preset } from '../lib/ipc/presets';
import { diffPresets } from '../lib/presets/resolve';

interface Props {
  from: Preset;
  to: Preset;
  onCancel: () => void;
  onConfirm: () => void;
}

export function PresetDiffDialog({ from, to, onCancel, onConfirm }: Props) {
  const rows = diffPresets(from, to);
  return (
    <div className="preset-diff-mask" onClick={onCancel}>
      <div className="preset-diff" onClick={(e) => e.stopPropagation()}>
        <div className="preset-diff-h">切换到「{to.name}」档</div>
        <p className="preset-diff-desc">{to.description}</p>
        <table className="preset-diff-t">
          <thead>
            <tr>
              <th>维度</th>
              <th>{from.name}档(当前)</th>
              <th>{to.name}档</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key} className={r.changed ? 'chg' : ''}>
                <td className="k">{r.label}</td>
                <td>{r.from}</td>
                <td>{r.to}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="preset-diff-note">
          钉住到本会话并即时生效:下一轮消息的工具下发与上下文阈值按新预设;
          审批默认仅为建议值,实际仍以会话权限档为准。
        </p>
        <div className="preset-diff-f">
          <button type="button" className="btn-ghost" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="btn-primary" onClick={onConfirm}>
            确认切换
          </button>
        </div>
      </div>
    </div>
  );
}
