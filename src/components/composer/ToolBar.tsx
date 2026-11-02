import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import type { ReasoningCapability } from '../../lib/ipc';
import { THINKING_LEVEL_LABEL, effortHint } from '../../lib/thinkingHint';
import type { ThinkingLevel, ThinkingSelection } from '../../stores/thinking';
import {
  IconBrain,
  IconChevronDown,
  IconGlobe,
  IconPlus,
  IconSend,
  IconStop,
} from '../icons';

const ACCEPT =
  '.docx,.xlsx,.xls,.pdf,.txt,.md,.markdown,.csv,.tsv,.log,.json,.yaml,.yml,.png,.jpg,.jpeg,.gif,.webp,.bmp';

export interface ThinkingControl {
  capability: ReasoningCapability;
  providerId: string;
  model: string;
  selection: ThinkingSelection;
  onSelect: (sel: ThinkingSelection) => void;
}

interface Props {
  disabled?: boolean;
  onPickFiles?: (files: File[]) => void;
  webSearch?: boolean;
  onToggleWebSearch?: () => void;
  thinking?: ThinkingControl | null;
  sending?: boolean;
  onStop?: () => void;
  /** 发送按钮是否可用(原文非空或有附件,且未 disabled) */
  canSend: boolean;
  onSend: () => void;
  /** 本会话模型选择器:置于工具条右端、发送按钮之前 */
  model?: { name: string; ready: boolean; onOpen: () => void } | null;
  /** 追加到工具条的自定义 chip(智能体 / 会话设置等) */
  children?: ReactNode;
}

/** 输入框底部工具条:附件 / 联网 / 思考强度 / 自定义 chip / 发送或停止。 */
export default function ToolBar({
  disabled,
  onPickFiles,
  webSearch,
  onToggleWebSearch,
  thinking,
  sending,
  onStop,
  canSend,
  onSend,
  model,
  children,
}: Props) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [thinkMenu, setThinkMenu] = useState(false);
  /**
   * 思考强度浮层的定位:固定定位,左边缘与「思考」chip 对齐(窄窗内自动钳制在窗口内)。
   * 这里实测 chip 的矩形,换算出浮层距窗口底部的距离、左偏移与可用最大高度,
   * 保证窄窗/矮窗都能完整显示(放不下时内部滚动),不被窗口左缘裁掉。
   */
  const thinkBtnRef = useRef<HTMLButtonElement>(null);
  const [thinkPos, setThinkPos] = useState({ bottom: 8, maxH: 240, left: 8 });
  /**
   * 工具条紧凑态:工作台态 chip 变多(联网/思考/权限/智能体/模式),再加上右侧
   * 「模型 + 发送」,输入条可用宽度不足时会把 mode chip 挤到第二行。
   * 这里按 chip 实测宽度(而非视口宽度)切换 compact:CSS 里收掉模型名,
   * 保证「模型 + 发送」与 chip 始终同一行(plan 5.2 回归修复)。
   */
  const barRef = useRef<HTMLDivElement>(null);
  const chipsRef = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);

  const syncThinkPos = () => {
    const r = thinkBtnRef.current?.getBoundingClientRect();
    if (!r) return;
    // 浮层底边贴在 chip 顶边上方 8px;可用高度 = 窗口顶部到浮层底边的距离
    const bottom = Math.max(8, window.innerHeight - r.top + 8);
    // 左边缘从 chip 左边缘起算,超出窗口时向回收,最少留 8px
    const menuW = Math.min(300, window.innerWidth - 24);
    const left = Math.max(8, Math.min(r.left, window.innerWidth - menuW - 8));
    setThinkPos({ bottom, maxH: Math.max(140, window.innerHeight - bottom - 12), left });
  };

  useEffect(() => {
    if (!thinkMenu) return;
    syncThinkPos();
    window.addEventListener('resize', syncThinkPos);
    return () => window.removeEventListener('resize', syncThinkPos);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thinkMenu]);

  /**
   * 紧凑态判定:按 chip 实测总宽 + 右侧「模型(带名) + 发送」预留 200px 与工具条宽比较。
   * chip 总宽不受紧凑态影响(只收模型名),所以判定是稳定的,不会反复切换。
   * ★ 不能用视口媒体查询:composer 自身有 720px 上限,窄视口下窗口宽度与输入条宽度无关。
   */
  const measureCompact = () => {
    const bar = barRef.current;
    const chips = chipsRef.current;
    if (!bar || !chips) return;
    const scale =
      Number.parseFloat(
        getComputedStyle(bar).getPropertyValue('--ui-scale').trim() || '1',
      ) || 1;
    const gap = Number.parseFloat(getComputedStyle(chips).columnGap) || 0;
    let need = gap * Math.max(0, chips.children.length - 1);
    for (const el of Array.from(chips.children)) need += (el as HTMLElement).offsetWidth;
    const on = need + 230 * scale > bar.clientWidth;
    setCompact(on);
  };
  useLayoutEffect(() => measureCompact);
  useEffect(() => {
    const bar = barRef.current;
    if (!bar) return;
    const ro = new ResizeObserver(() => measureCompact());
    ro.observe(bar);
    return () => ro.disconnect();
  }, []);

  return (
    <div ref={barRef} className={`composer-bar${compact ? ' compact' : ''}`}>
      <div className="toolbar-chips" ref={chipsRef}>
      {onPickFiles && (
        <>
          <button
            type="button"
            className="tool-plus"
            onClick={() => fileRef.current?.click()}
            disabled={disabled}
            title="添加附件(docx / xlsx / pdf / 图片)"
            aria-label="添加附件"
          >
            <IconPlus size={15} />
          </button>
          <input
            ref={fileRef}
            type="file"
            multiple
            accept={ACCEPT}
            style={{ display: 'none' }}
            onChange={(e) => {
              const list = e.target.files;
              if (list?.length) onPickFiles(Array.from(list));
              e.target.value = '';
            }}
          />
        </>
      )}
      {onToggleWebSearch && (
        <button
          type="button"
          className={`tool-chip ${webSearch ? 'on' : ''}`}
          onClick={onToggleWebSearch}
          disabled={disabled}
          title={webSearch ? '联网搜索已开启(点击关闭)' : '开启联网搜索'}
        >
          <IconGlobe size={13} />
          <span>联网</span>
        </button>
      )}
      {thinking && thinking.capability.enabled && (() => {
        const cap = thinking.capability;
        const sel = thinking.selection;
        // 仅开关模型(如 Kimi K2.6 / GLM-4.5):无档位差异,四档控件不可用
        const noLevels = !cap.levels;
        // 不可关模型(如 Kimi K3 / GLM-5.3):锁定为开
        const locked = !cap.can_disable;
        const chipTitle = locked
          ? noLevels
            ? '该模型始终开启思考'
            : '该模型始终开启思考,可选择强度'
          : noLevels
            ? sel.on ? '思考已开启(点击关闭)' : '开启思考'
            : '选择思考强度(展示实际发送参数)';
        return (
          <span className="pin-wrap">
            <button
              ref={thinkBtnRef}
              type="button"
              className={`tool-chip ${sel.on ? 'on' : ''}`}
              onClick={() => {
                if (noLevels) {
                  if (cap.can_disable) thinking.onSelect({ ...sel, on: !sel.on });
                  return;
                }
                if (!thinkMenu) syncThinkPos();
                setThinkMenu((v) => !v);
              }}
              disabled={disabled || (locked && noLevels)}
              title={chipTitle}
            >
              <IconBrain size={13} />
              <span className="ctl-chip-text">
                思考{!noLevels && sel.on ? ` · ${THINKING_LEVEL_LABEL[sel.level]}` : ''}
              </span>
              {!noLevels && <IconChevronDown size={11} />}
            </button>
            {thinkMenu && !noLevels && (
              <>
                <div className="pin-backdrop" onClick={() => setThinkMenu(false)} />
                <div
                  className="pin-menu think-menu"
                  style={{ bottom: thinkPos.bottom, left: thinkPos.left, maxHeight: thinkPos.maxH }}
                >
                  <div className="pin-menu-title">思考强度(实际发送参数)</div>
                  {(['light', 'medium', 'high', 'ultra'] as ThinkingLevel[]).map((lv) => (
                    <button
                      key={lv}
                      type="button"
                      className={`think-item ${sel.on && sel.level === lv ? 'active' : ''}`}
                      onClick={() => {
                        thinking.onSelect({ on: true, level: lv });
                        setThinkMenu(false);
                      }}
                    >
                      <span>{THINKING_LEVEL_LABEL[lv]}</span>
                      <span className="hint">{effortHint(thinking.providerId, thinking.model, lv)}</span>
                    </button>
                  ))}
                  {cap.can_disable && (
                    <>
                      <div className="think-divider" />
                      <button
                        type="button"
                        className={`think-item ${!sel.on ? 'active' : ''}`}
                        onClick={() => {
                          thinking.onSelect({ ...sel, on: false });
                          setThinkMenu(false);
                        }}
                      >
                        <span>关闭思考</span>
                      </button>
                    </>
                  )}
                </div>
              </>
            )}
          </span>
        );
      })()}
      {children}
      </div>
      <div className="toolbar-actions">
      {model && (
        <button
          type="button"
          className="composer-model"
          onClick={model.onOpen}
          disabled={disabled}
          title="切换本会话使用的模型"
        >
          <span className={`stat-dot ${model.ready ? 'ok' : 'warn'}`} />
          <span className="model-name">{model.name}</span>
          <IconChevronDown size={12} />
        </button>
      )}
      {sending && onStop ? (
        <button type="button" className="stop-btn" onClick={onStop} title="停止">
          <IconStop size={14} />
        </button>
      ) : (
        <button className="send" onClick={onSend} disabled={!canSend} title="发送">
          <IconSend size={18} />
        </button>
      )}
      </div>
    </div>
  );
}
