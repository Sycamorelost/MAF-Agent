//! 运行时预设 chip(P5 / D18):展示当前生效档位(会话钉住 > Agent 模板 > 全局默认),
//! 点击切换本会话钉住;切换到不同档前弹 PresetDiffDialog 确认差异(D19 显式确认)。
//! 放在通用 components(见 val.md §5.8 文件拆分),工作台与后续普通会话均可复用。

import { useEffect, useRef, useState } from 'react';
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

interface Props {
  sessionId: string;
  onError: (msg: string) => void;
}

export function PresetChip({ sessionId, onError }: Props) {
  const session = useSessionsStore((s) => s.sessions.find((x) => x.id === sessionId));
  const setPreset = useSessionsStore((s) => s.setPreset);
  const [info, setInfo] = useState<SessionPresetInfo | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [open, setOpen] = useState(false);
  const [diffTarget, setDiffTarget] = useState<Preset | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listPresets().then(setPresets).catch(() => {});
  }, []);

  // 会话切换或钉住状态变化后重拉解析结果
  useEffect(() => {
    resolveSessionPreset(sessionId).then(setInfo).catch(() => setInfo(null));
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

  if (!info) return null;
  const current = info.preset;
  const pinned = info.pinned_id != null;

  const pick = (p: Preset) => {
    setOpen(false);
    if (p.id === current.id) return;
    setDiffTarget(p);
  };

  const resetInherit = async () => {
    setOpen(false);
    if (!pinned) return;
    if (!window.confirm('恢复为跟随默认(Agent 模板 > 全局设置)?仅影响当前会话。')) return;
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
        onClick={() => setOpen((v) => !v)}
      >
        <IconGauge size={12} />
        {current.name}
        {pinned && <i className="perm-dot" />}
      </button>
      {open && (
        <div className="perm-menu">
          <div className="perm-menu-note">
            {current.description}(来源:{PRESET_SOURCE_LABEL[info.source] ?? info.source})
          </div>
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
