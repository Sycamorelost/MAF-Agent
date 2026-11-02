//! 运行时预设 chip(P5 / D18):展示当前生效档位(会话钉住 > Agent 模板 > 全局默认),
//! 点击切换本会话钉住;切换到不同档前弹 PresetDiffDialog 确认差异(D19 显式确认)。
//! 放在通用 components(见 val.md §5.8 文件拆分),工作台与后续普通会话均可复用。

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { IconGauge } from './icons';
import {
  listPresets,
  resolveSessionPreset,
  type Preset,
  type SessionPresetInfo,
} from '../lib/ipc/presets';
import { PRESET_SOURCE_LABEL } from '../lib/presets/resolve';
import { useSessionsStore } from '../stores/sessions';
import { PresetDiffDialog } from './PresetDiffDialog';
import { uiConfirm } from './confirmService';

interface Props {
  sessionId: string;
  onError: (msg: string) => void;
}

export function PresetChip({ sessionId, onError }: Props) {
  const session = useSessionsStore((s) => s.sessions.find((x) => x.id === sessionId));
  const setPreset = useSessionsStore((s) => s.setPreset);
  const [info, setInfo] = useState<SessionPresetInfo | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [presetsLoading, setPresetsLoading] = useState(false);
  const [presetsError, setPresetsError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [alignLeft, setAlignLeft] = useState(false);
  const [diffTarget, setDiffTarget] = useState<Preset | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  /** 拉取可选预设;失败时保留错误供菜单内重试(禁止静默吞掉异常)。 */
  const loadPresets = useCallback(async () => {
    setPresetsLoading(true);
    setPresetsError(null);
    try {
      setPresets(await listPresets());
    } catch (e) {
      setPresetsError(e instanceof Error ? e.message : String(e));
    } finally {
      setPresetsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPresets();
  }, [loadPresets]);

  // 会话切换或钉住状态变化后重拉解析结果;失败时保留上次结果并明确反馈
  useEffect(() => {
    let cancelled = false;
    resolveSessionPreset(sessionId)
      .then((v) => {
        if (!cancelled) setInfo(v);
      })
      .catch((e) => {
        if (cancelled) return;
        onError(`加载运行时预设失败:${e instanceof Error ? e.message : String(e)}`);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, session?.preset_id, session?.preset_version]);

  // 点击外部收起菜单
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  // 防左溢出:菜单默认右对齐 chip;chip 靠近窗口左缘时(如会话头部最左侧)
  // 右锚会让 260px 菜单伸出左缘,改为左对齐锚定。预设列表后到时宽度可能变化,需重测。
  useLayoutEffect(() => {
    if (!open) return;
    const wrap = wrapRef.current;
    if (!wrap) return;
    const chip = wrap.getBoundingClientRect();
    const menu = wrap.querySelector<HTMLElement>('.perm-menu');
    const w = menu?.offsetWidth ?? 260;
    // 右对齐时菜单左缘 = chip 右缘 - 菜单宽;小于安全边距即翻转
    setAlignLeft(chip.right - w < 8);
  }, [open, presets.length]);

  if (!info) return null;
  const current = info.preset;
  const pinned = info.pinned_id != null;

  const toggleOpen = () => {
    const next = !open;
    // 打开时若列表为空(首次加载失败/尚未返回),重试一次,避免菜单空白无反馈
    if (next && presets.length === 0 && !presetsLoading) void loadPresets();
    setOpen(next);
  };

  const pick = (p: Preset) => {
    setOpen(false);
    if (p.id === current.id) return;
    setDiffTarget(p);
  };

  const resetInherit = async () => {
    setOpen(false);
    if (!pinned) return;
    if (
      !(await uiConfirm({
        title: '恢复默认预设',
        message: '恢复为跟随默认(Agent 模板 > 全局设置)?仅影响当前会话。',
      }))
    )
      return;
    try {
      await setPreset(sessionId, null);
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    }
  };

  const confirmSwitch = async () => {
    const target = diffTarget;
    setDiffTarget(null);
    if (!target) return;
    try {
      await setPreset(sessionId, target.id);
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="perm-chip-wrap" ref={wrapRef}>
      <button
        type="button"
        className="preset-chip"
        title={`运行时预设:${current.description}\n来源:${PRESET_SOURCE_LABEL[info.source] ?? info.source}`}
        onClick={toggleOpen}
      >
        <IconGauge size={12} />
        <span className="ctl-chip-text">{current.name}</span>
        {pinned && <i className="perm-dot" />}
      </button>
      {open && (
        <div className={alignLeft ? 'perm-menu align-left' : 'perm-menu'}>
          <div className="perm-menu-note">
            {current.description}(来源:{PRESET_SOURCE_LABEL[info.source] ?? info.source})
          </div>
          {presetsLoading && presets.length === 0 && (
            <div className="perm-menu-hint">正在加载预设…</div>
          )}
          {presetsError && (
            <button type="button" className="danger" onClick={() => void loadPresets()}>
              加载失败,点击重试
            </button>
          )}
          {presets.map((p) => (
            <button
              key={p.id}
              type="button"
              className={current.id === p.id ? 'on' : ''}
              onClick={() => pick(p)}
            >
              {p.name}
            </button>
          ))}
          {pinned && (
            <button type="button" onClick={() => void resetInherit()}>
              恢复继承
            </button>
          )}
        </div>
      )}
      {diffTarget && (
        <PresetDiffDialog
          from={current}
          to={diffTarget}
          onCancel={() => setDiffTarget(null)}
          onConfirm={() => void confirmSwitch()}
        />
      )}
    </div>
  );
}
