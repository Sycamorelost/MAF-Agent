//! 设置 · 外观 —— 字体大小缩放 + 预制主题切换 + 应用标识自定义。
//! 字体大小只缩放字号 token(-font-*),不改布局间距;主题切换通过 data-theme 覆盖 CSS 变量。
//! 应用标识仅影响窗口标题/任务栏/托盘,不影响关于页(关于页固定 "MAF Agent")。

import { useEffect, useState } from 'react';
import {
  applyUiScale,
  loadUiScale,
  UI_SCALE_DEFAULT,
  UI_SCALE_MAX,
  UI_SCALE_MIN,
} from '../../lib/uiScale';
import { useThemeStore, THEMES, type ThemeKey } from '../../stores/theme';
import * as ipc from '../../lib/ipc';

/** 预设档位(百分比)。 */
const PRESETS: { pct: number; label: string }[] = [
  { pct: 90, label: '小' },
  { pct: 100, label: '标准' },
  { pct: 110, label: '大' },
  { pct: 120, label: '特大' },
];

export function AppearanceView() {
  const [pct, setPct] = useState(() => Math.round(loadUiScale() * 100));
  const theme = useThemeStore((s) => s.theme);
  const applyTheme = useThemeStore((s) => s.applyTheme);

  // 应用标识:自定义名称
  const [appName, setAppName] = useState('');
  const [nameSaved, setNameSaved] = useState(false);

  useEffect(() => {
    ipc.getSettings()
      .then((s) => setAppName(s.custom_app_name ?? ''))
      .catch(() => {});
  }, []);

  const onChange = (next: number) => {
    const applied = Math.round(applyUiScale(next / 100) * 100);
    setPct(applied);
  };

  const onPickTheme = (t: ThemeKey) => {
    applyTheme(t);
  };

  const onSaveAppName = async () => {
    const settings = await ipc.getSettings();
    const next = { ...settings, custom_app_name: appName.trim() || null };
    await ipc.saveSettings(next);
    setNameSaved(true);
    setTimeout(() => setNameSaved(false), 2000);
  };

  return (
    <>
      <h2>字体大小</h2>
      <p className="set-desc">
        按比例调整整个界面的字号(含对话正文、设置页、侧栏等)。只放大文字,不改变布局间距,
        因此不会出现排版错位;调整立即生效并记住,下次启动沿用。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          字号缩放
          <span className="st">{pct}%</span>
        </div>
        <div className="set-card-b">
          <div className="set-actions" style={{ flexWrap: 'wrap' }}>
            {PRESETS.map((p) => (
              <button
                key={p.pct}
                className={`btn btn-xs${pct === p.pct ? ' btn-primary' : ''}`}
                onClick={() => onChange(p.pct)}
              >
                {p.label} · {p.pct}%
              </button>
            ))}
            <span className="sp" />
            <button
              className="btn btn-xs"
              disabled={pct === Math.round(UI_SCALE_DEFAULT * 100)}
              onClick={() => onChange(Math.round(UI_SCALE_DEFAULT * 100))}
            >
              恢复默认
            </button>
          </div>

          <div className="set-row" style={{ marginTop: 'var(--space-3)' }}>
            <label className="lbl">自定义</label>
            <input
              type="range"
              min={Math.round(UI_SCALE_MIN * 100)}
              max={Math.round(UI_SCALE_MAX * 100)}
              step={5}
              value={pct}
              onChange={(e) => onChange(Number(e.target.value))}
              style={{ flex: 1, maxWidth: 320 }}
            />
            <span className="set-hint">
              {Math.round(UI_SCALE_MIN * 100)}% – {Math.round(UI_SCALE_MAX * 100)}%
            </span>
          </div>

          <div className="set-hint" style={{ marginTop: 'var(--space-3)' }}>
            预览:这是正文的显示效果,模型回复与工具结果都按此字号呈现。
          </div>
          <div
            style={{
              marginTop: 'var(--space-2)',
              padding: 'var(--space-3)',
              border: '1px solid var(--border)',
              borderRadius: 'var(--radius-s)',
              fontSize: 'var(--font-body)',
              lineHeight: 1.7,
            }}
          >
            正文示例 · MAF Agent 会在这里输出回答。字号随上方设置实时变化。
            <div className="set-hint" style={{ marginTop: 'var(--space-1)' }}>
              次要说明文字示例(--font-caption)
            </div>
          </div>
        </div>
      </div>

      <h2 style={{ marginTop: 'var(--space-6)' }}>主题</h2>
      <p className="set-desc">
        预制 6 套主题,一键切换界面配色。只改变背景与文字颜色,不影响按钮、输入框的尺寸与排版。
      </p>

      <div className="set-card">
        <div className="set-card-h">配色方案</div>
        <div className="set-card-b">
          <div className="set-actions" style={{ flexWrap: 'wrap', gap: 'var(--space-3)' }}>
            {THEMES.map((t) => {
              const active = theme === t.key;
              return (
                <button
                  key={t.key}
                  onClick={() => onPickTheme(t.key)}
                  style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    gap: 'var(--space-1)',
                    padding: 'var(--space-2)',
                    border: active ? '2px solid var(--brand)' : '1px solid var(--border)',
                    borderRadius: 'var(--radius-m)',
                    background: 'var(--bg-surface)',
                    cursor: 'pointer',
                    transition: 'border-color .15s',
                    minWidth: 72,
                  }}
                >
                  <span
                    style={{
                      width: 40,
                      height: 40,
                      borderRadius: 'var(--radius-s)',
                      background: t.swatch,
                      border: '1px solid var(--border-strong)',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: active ? 'var(--brand)' : 'transparent',
                      fontSize: 20,
                      transition: 'color .15s',
                    }}
                  >
                    ✓
                  </span>
                  <span
                    style={{
                      fontSize: 'var(--font-micro)',
                      color: active ? 'var(--brand-deep)' : 'var(--text-2)',
                      fontWeight: active ? 600 : 400,
                    }}
                  >
                    {t.label}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <h2 style={{ marginTop: 'var(--space-6)' }}>应用标识</h2>
      <p className="set-desc">
        自定义应用在窗口标题、任务栏和系统托盘中显示的名称。
        注意:关于页始终显示「MAF Agent」,不可修改(版权归属)。
      </p>

      <div className="set-card">
        <div className="set-card-h">应用名称</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">显示名称</label>
            <input
              type="text"
              value={appName}
              onChange={(e) => setAppName(e.target.value)}
              placeholder="留空则使用默认名 MAF Agent"
              style={{ flex: 1, maxWidth: 320 }}
            />
            <button className="btn btn-xs" onClick={onSaveAppName} disabled={!appName.trim() && !appName}>
              保存
            </button>
            {nameSaved && <span className="set-hint" style={{ color: 'var(--brand)' }}>已保存</span>}
          </div>
          <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
            保存后窗口标题栏、Alt-Tab 与任务栏立即更新,无需重启。
          </div>
          <ul
            className="set-hint"
            style={{ margin: 'var(--space-2) 0 0', paddingLeft: 'var(--space-4)', listStyle: 'disc' }}
          >
            <li>系统托盘:菜单项「显示 …」与悬停提示在应用启动时读取一次,改名后需重启应用才更新。</li>
            <li>exe 文件名、开始菜单快捷方式与安装包名称:来自构建配置,改名不生效,需重新构建安装包。</li>
          </ul>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">应用图标</div>
        <div className="set-card-b">
          <div className="set-hint">
            暂无设置入口。窗口、任务栏、托盘与 exe 的图标都来自构建期打包的 src-tauri/icons 目录,
            重启应用不会改变;如需更换,请替换该目录下的图标文件后重新执行构建。
          </div>
        </div>
      </div>
    </>
  );
}
