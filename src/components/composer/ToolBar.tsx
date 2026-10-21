import { useRef, useState, type ReactNode } from 'react';
import type { ReasoningCapability } from '../../lib/ipc';
import { THINKING_LEVEL_LABEL, effortHint } from '../../lib/thinkingHint';
import type { ThinkingLevel, ThinkingSelection } from '../../stores/thinking';
import {
  IconAttach,
  IconBrain,
  IconChevronDown,
  IconGlobe,
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
  onPickFiles?: (files: FileList) => void;
  webSearch?: boolean;
  onToggleWebSearch?: () => void;
  thinking?: ThinkingControl | null;
  sending?: boolean;
  onStop?: () => void;
  /** 发送按钮是否可用(原文非空或有附件,且未 disabled) */
  canSend: boolean;
  onSend: () => void;
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
  children,
}: Props) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [thinkMenu, setThinkMenu] = useState(false);

  return (
    <div className="composer-bar">
      {onPickFiles && (
        <>
          <button
            type="button"
            className="tool-chip"
            onClick={() => fileRef.current?.click()}
            disabled={disabled}
            title="添加附件(docx / xlsx / pdf / 图片)"
          >
            <IconAttach size={13} />
            <span>附件</span>
          </button>
          <input
            ref={fileRef}
            type="file"
            multiple
            accept={ACCEPT}
            style={{ display: 'none' }}
            onChange={(e) => {
              if (e.target.files?.length) onPickFiles(e.target.files);
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
              <span>思考{!noLevels && sel.on ? ` · ${THINKING_LEVEL_LABEL[sel.level]}` : ''}</span>
              {!noLevels && <IconChevronDown size={11} />}
            </button>
            {thinkMenu && !noLevels && (
              <>
                <div className="pin-backdrop" onClick={() => setThinkMenu(false)} />
                <div className="pin-menu think-menu">
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
      <span className="spacer" />
      {sending && onStop ? (
        <button type="button" className="stop-btn" onClick={onStop} title="停止">
          <IconStop size={14} />
        </button>
      ) : (
        <button className="send" onClick={onSend} disabled={!canSend} title="发送">
          <IconSend size={14} />
        </button>
      )}
    </div>
  );
}
