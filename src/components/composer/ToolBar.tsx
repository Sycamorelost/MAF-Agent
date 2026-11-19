import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { ReasoningCapability } from '../../lib/ipc';
import { THINKING_LEVEL_LABEL, effortHint } from '../../lib/thinkingHint';
import type { ThinkingLevel, ThinkingSelection } from '../../stores/thinking';
import { usePinMenu } from './usePinMenu';
import {
  IconAttach,
  IconBrain,
  IconChevronDown,
  IconGlobe,
  IconPlus,
  IconSend,
  IconStop,
  IconUpload,
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
  /** 芯片只保留图标(小窗紧凑模式) */
  iconOnly?: boolean;
  /** 追加到工具条的自定义 chip(智能体 / 会话设置等) */
  children?: ReactNode;
  /** 打开「从附件中心导入」选择器;未提供时附件按钮保持直接选文件 */
  onOpenAttachmentCenter?: () => void;
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
  iconOnly,
  children,
  onOpenAttachmentCenter,
}: Props) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [thinkMenu, setThinkMenu] = useState(false);
  /** 附件来源浮层:本地上传 / 从附件中心导入 */
  const [srcMenu, setSrcMenu] = useState(false);
  /**
   * 浮层定位:实测锚点矩形,换算 fixed 坐标与可用高度(见 usePinMenu)。
   * 两个浮层都渲染到 document.body,避免被顶部任务条或输入区容器裁切。
   */
  const thinkBtnRef = useRef<HTMLButtonElement>(null);
  const thinkMenuRef = useRef<HTMLDivElement>(null);
  const think = usePinMenu(thinkMenu, thinkBtnRef, thinkMenuRef, {
    minHeight: 160,
    fallbackWidth: 300,
  });
  const srcBtnRef = useRef<HTMLButtonElement>(null);
  const srcMenuRef = useRef<HTMLDivElement>(null);
  const src = usePinMenu(srcMenu, srcBtnRef, srcMenuRef, {
    minHeight: 120,
    fallbackWidth: 176,
  });
  /**
   * 工具条紧凑态:工作台态 chip 变多(联网/思考/权限/智能体/模式),再加上右侧
   * 「模型 + 发送」,输入条可用宽度不足时会把 mode chip 挤到第二行。
   * 这里按 chip 实测宽度(而非视口宽度)切换 compact:CSS 里收掉模型名,
   * 保证「模型 + 发送」与 chip 始终同一行(plan 5.2 回归修复)。
   */
  const barRef = useRef<HTMLDivElement>(null);
  const chipsRef = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);

  /**
   * 紧凑态判定:按 chip 实测总宽 + 右侧「模型(带名) + 发送」预留 200px 与工具条宽比较。
   * chip 总宽不受紧凑态影响(只收模型名),所以判定是稳定的,不会反复切换。
   * [注意] 不能用视口媒体查询:composer 自身有 720px 上限,窄视口下窗口宽度与输入条宽度无关。
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
          <span className="attach-src-wrap">
            <button
              ref={srcBtnRef}
              type="button"
              className="tool-plus"
              onClick={() => {
                // 提供了附件中心入口才展开来源选择;否则保持原有直接选文件行为
                if (onOpenAttachmentCenter) setSrcMenu((v) => !v);
                else fileRef.current?.click();
              }}
              disabled={disabled}
              title="添加附件(docx / xlsx / pdf / 图片 / 附件中心)"
              aria-label="添加附件"
            >
              <IconPlus size={15} />
            </button>
            {srcMenu && onOpenAttachmentCenter && createPortal(
              <>
                <div className="pin-backdrop" onClick={() => setSrcMenu(false)} />
                <div
                  ref={srcMenuRef}
                  className={`pin-menu pin-menu-portal attach-src-menu ${src.pos.placement}`}
                  style={{
                    top: src.pos.top,
                    bottom: src.pos.bottom,
                    left: src.pos.left,
                    maxHeight: src.pos.maxHeight,
                  }}
                >
                  <div className="pin-menu-title">添加附件</div>
                  <button
                    type="button"
                    className="attach-src-item"
                    onClick={() => {
                      setSrcMenu(false);
                      fileRef.current?.click();
                    }}
                  >
                    <IconUpload size={14} />
                    <span>从电脑上传</span>
                  </button>
                  <button
                    type="button"
                    className="attach-src-item"
                    onClick={() => {
                      setSrcMenu(false);
                      onOpenAttachmentCenter();
                    }}
                  >
                    <IconAttach size={14} />
                    <span>从附件中心导入</span>
                  </button>
                </div>
              </>,
              document.body,
            )}
          </span>
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
          {!iconOnly && <span>联网</span>}
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
                setThinkMenu((v) => !v);
              }}
              disabled={disabled || (locked && noLevels)}
              title={chipTitle}
            >
              <IconBrain size={13} />
              {!iconOnly && (
                <>
                  <span className="ctl-chip-text">
                    思考{!noLevels && sel.on ? ` · ${THINKING_LEVEL_LABEL[sel.level]}` : ''}
                  </span>
                  {!noLevels && <IconChevronDown size={11} />}
                </>
              )}
            </button>
            {thinkMenu && !noLevels && (
              createPortal(
                <>
                  <div className="pin-backdrop" onClick={() => setThinkMenu(false)} />
                  <div
                    ref={thinkMenuRef}
                    className={`pin-menu pin-menu-portal think-menu ${think.pos.placement}`}
                    style={{
                      top: think.pos.top,
                      bottom: think.pos.bottom,
                      left: think.pos.left,
                      maxHeight: think.pos.maxHeight,
                    }}
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
                </>,
                document.body,
              )
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
