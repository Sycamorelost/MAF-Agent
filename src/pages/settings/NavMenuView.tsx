//! 设置 · 导航菜单可见性视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import { IconCheck } from '../../components/icons';
import { NAV_VISIBILITY } from './constants';

export interface NavMenuViewProps {
  /** 已隐藏的导航项 key */
  hiddenNav: string[];
  settingsSaved: boolean;
  savingSettings: boolean;
  onToggleNavItem: (key: string) => void;
  onSaveSettings: () => void;
}

export function NavMenuView({
  hiddenNav,
  settingsSaved,
  savingSettings,
  onToggleNavItem,
  onSaveSettings,
}: NavMenuViewProps) {
  return (
    <>
      <h2>导航菜单</h2>
      <p className="set-desc">
        控制标题栏 ⋮ 下拉菜单中显示的功能入口。「对话」与「设置」固定保留,其余项可自行决定是否展示;
        隐藏后不影响功能本身,仍可从会话、搜索等入口访问对应内容。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          功能入口
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />
              已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          {NAV_VISIBILITY.map((it) => {
            const shown = !hiddenNav.includes(it.key);
            return (
              <div className="set-row" key={it.key}>
                <label className="lbl">{it.label}</label>
                <button
                  type="button"
                  role="switch"
                  aria-checked={shown}
                  className={`set-switch${shown ? ' on' : ''}`}
                  onClick={() => onToggleNavItem(it.key)}
                >
                  <span className="knob" />
                </button>
                <span className="set-hint">{shown ? '在菜单中显示' : '已隐藏'}</span>
              </div>
            );
          })}
          <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
            对话与设置始终显示,无需设置。
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSettings} disabled={savingSettings}>
              保存界面设置
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
