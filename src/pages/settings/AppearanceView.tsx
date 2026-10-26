//! 设置 · 外观 —— 字体大小缩放 + 预制主题切换 + 自定义壁纸。
//! 字体大小只缩放字号 token(-font-*),不改布局间距;主题切换通过 data-theme 覆盖 CSS 变量;
//! 壁纸仅替换 .window 背景层(用户图 + 主题色渐变叠加),不影响任何布局属性。

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
  const setWallpaper = useThemeStore((s) => s.setWallpaper);
  const setWallpaperCover = useThemeStore((s) => s.setWallpaperCover);
  // 缩略图直接复用 store 的 blob URL,不再把 ~MB 级 base64 字符串常驻本组件 state
  const wallpaperBlobUrl = useThemeStore((s) => s.wallpaperBlobUrl);

  // 壁纸状态:路径(展示/持久化)+ 遮罩不透明度;渲染走 store 的 blob URL
  const [wallpaper, setWallpaperState] = useState<string | null>(null);
  const [cover, setCover] = useState(0);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // 进入页面时拉取当前设置中的壁纸路径;store 已有 blob(启动已挂载)则零额外读取,
  // 否则读一次文件并调用 setWallpaper 顺带自愈背景
  useEffect(() => {
    ipc
      .getSettings()
      .then(async (s) => {
        setCover(s.wallpaper_cover ?? 0);
        setWallpaperCover(s.wallpaper_cover ?? 0);
        if (s.wallpaper_path) {
          setWallpaperState(s.wallpaper_path);
          if (!useThemeStore.getState().wallpaperBlobUrl) {
            try {
              const dataUrl = await ipc.readWallpaper(s.wallpaper_path);
              setWallpaper(s.wallpaper_path, dataUrl);
            } catch {
              // 文件已被删除等情况:忽略,保持空白
            }
          }
        }
      })
      .catch(() => {});
  }, [setWallpaper]);

  const onChange = (next: number) => {
    const applied = Math.round(applyUiScale(next / 100) * 100);
    setPct(applied);
  };

  /** 上传壁纸:选图 → 后端校验拷贝 → 写入 AppSettings → 读 data URL → 挂载渲染。 */
  const onUpload = async () => {
    setErr(null);
    try {
      const picked = await ipc.pickWallpaperFile();
      if (!picked) return;
      setBusy(true);
      const stored = await ipc.saveThemeWallpaper(picked);
      // 写回 AppSettings 持久化
      const cur = await ipc.getSettings();
      await ipc.saveSettings({ ...cur, wallpaper_path: stored });
      // 读取为 data URL 后再渲染(WebView2 不支持 file://)
      const dataUrl = await ipc.readWallpaper(stored);
      setWallpaperState(stored);
      setWallpaper(stored, dataUrl);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  /** 移除壁纸:清空 AppSettings 字段 + 卸载渲染层。 */
  const onRemove = async () => {
    setErr(null);
    try {
      setBusy(true);
      const cur = await ipc.getSettings();
      await ipc.saveSettings({ ...cur, wallpaper_path: null });
      setWallpaperState(null);
      setWallpaper(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  /** 切换主题:主题与壁纸互斥,若壁纸激活则先移除壁纸(含持久化),再应用主题。 */
  const onPickTheme = async (t: ThemeKey) => {
    if (wallpaper) {
      await onRemove();
    }
    applyTheme(t);
  };
  const onCoverChange = async (v: number) => {
    setCover(v);
    setWallpaperCover(v);
    try {
      const cur = await ipc.getSettings();
      await ipc.saveSettings({ ...cur, wallpaper_cover: v });
    } catch {
      // 持久化失败不影响即时渲染
    }
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
        <br />
        <span style={{ color: 'var(--text-3)' }}>主题与自定义壁纸互斥:选择主题将自动关闭壁纸。</span>
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

      <h2 style={{ marginTop: 'var(--space-6)' }}>自定义壁纸</h2>
      <p className="set-desc">
        上传一张图片作为界面背景(仅替换背景层,不影响按钮与排版)。图片会叠加一层白色半透明渐变,
        保证文字始终可读。支持 JPG / PNG / WebP,单张不超过 8 MB。
        <br />
        <span style={{ color: 'var(--text-3)' }}>壁纸与主题互斥:开启壁纸将挂起主题配色,使用默认纯白表面。</span>
      </p>

      <div className="set-card">
        <div className="set-card-h">壁纸</div>
        <div className="set-card-b">
          {wallpaper ? (
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 'var(--space-3)',
                flexWrap: 'wrap',
              }}
            >
              <div
                style={{
                  width: 120,
                  height: 72,
                  borderRadius: 'var(--radius-s)',
                  border: '1px solid var(--border)',
                  backgroundImage: wallpaperBlobUrl
                    ? `url("${wallpaperBlobUrl}")`
                    : 'var(--bg-surface)',
                  backgroundSize: 'cover',
                  backgroundPosition: 'center',
                  flexShrink: 0,
                }}
              />
              <div style={{ flex: 1, minWidth: 200 }}>
                <div
                  className="set-hint"
                  style={{
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    maxWidth: 320,
                  }}
                  title={wallpaper}
                >
                  {wallpaper}
                </div>
                <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
                  <button
                    className="btn btn-xs"
                    disabled={busy}
                    onClick={onUpload}
                  >
                    {busy ? '处理中…' : '更换图片'}
                  </button>
                  <button
                    className="btn btn-xs btn-ghost"
                    disabled={busy}
                    onClick={onRemove}
                  >
                    移除壁纸
                  </button>
                </div>
              </div>
            </div>
          ) : (
            <div className="set-actions">
              <button className="btn btn-xs" disabled={busy} onClick={onUpload}>
                {busy ? '处理中…' : '上传壁纸'}
              </button>
              <span className="set-hint">未设置壁纸时使用主题纯色背景</span>
            </div>
          )}
          {err && (
            <div
              className="set-hint"
              style={{ marginTop: 'var(--space-2)', color: 'var(--danger, #d4380d)' }}
            >
              {err}
            </div>
          )}

          <div style={{ marginTop: 'var(--space-4)' }}>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                marginBottom: 'var(--space-1)',
              }}
            >
              <span style={{ fontSize: 'var(--font-caption)', color: 'var(--text-2)' }}>
                遮罩不透明度
              </span>
              <span
                style={{
                  fontSize: 'var(--font-micro)',
                  color: 'var(--text-3)',
                  fontVariantNumeric: 'tabular-nums',
                }}
              >
                {cover}%
              </span>
            </div>
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={cover}
              onChange={(e) => onCoverChange(Number(e.target.value))}
              disabled={!wallpaper}
              style={{ width: '100%' }}
            />
            <div
              className="set-hint"
              style={{ marginTop: 'var(--space-1)' }}
            >
              0% = 壁纸完全可见,100% = 主题默认遮罩(保证文字可读)
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
