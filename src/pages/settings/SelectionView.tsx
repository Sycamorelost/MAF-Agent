//! 设置 · 划词提问视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import * as ipc from '../../lib/ipc';
import { IconCheck } from '../../components/icons';

export interface SelectionViewProps {
  settings: ipc.AppSettings | null;
  settingsSaved: boolean;
  savingSettings: boolean;
  onPatchSettings: (patch: Partial<ipc.AppSettings>) => void;
  onSaveSelection: () => void;
}

export function SelectionView({
  settings,
  settingsSaved,
  savingSettings,
  onPatchSettings,
  onSaveSelection,
}: SelectionViewProps) {
  return (
    <>
      <h2>划词提问</h2>
      <p className="set-desc">
        在任意应用中选中文本后连按两次 Ctrl+C,主窗口会被唤起并新建会话,选中文字自动填入输入框,按回车即可提问。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          触发开关
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">划词提问</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.selection_ask_enabled ?? false}
              className={`set-switch${settings?.selection_ask_enabled ? ' on' : ''}`}
              onClick={() =>
                onPatchSettings({ selection_ask_enabled: !settings?.selection_ask_enabled })
              }
            >
              <span className="knob" />
            </button>
            <span className="set-hint">关闭后连按两次 Ctrl+C 不再唤起主窗口</span>
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSelection} disabled={savingSettings}>
              保存划词设置
            </button>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">主窗口 / 托盘</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">最小化到托盘</label>
            <button
              type="button"
              role="switch"
              aria-checked={settings?.minimize_to_tray ?? false}
              className={`set-switch${settings?.minimize_to_tray ? ' on' : ''}`}
              onClick={() => onPatchSettings({ minimize_to_tray: !settings?.minimize_to_tray })}
            >
              <span className="knob" />
            </button>
            <span className="set-hint">
              默认关:点 × 退出应用,最小化保留任务栏图标;开启后点 × 与最小化都隐藏到托盘
            </span>
          </div>
        </div>
      </div>
    </>
  );
}
