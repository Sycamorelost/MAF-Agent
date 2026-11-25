import { useCallback, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import Composer from '../../components/Composer';
import AttachmentCenterPicker from '../../components/composer/AttachmentCenterPicker';
import { agentIconComponent } from '../../components/icons';
import * as ipc from '../../lib/ipc';
import type {
  AgentTemplate,
  AttachmentMeta,
  ReasoningCapability,
  SessionRow,
} from '../../lib/ipc';
import type { QuoteRef } from '../../lib/quote';
import type { SkippedFile } from '../../lib/attachmentDrop';
import { DEFAULT_THINKING_LEVEL, type ThinkingSelection } from '../../stores/thinking';

interface ChatComposerAreaProps {
  sessionId: string;
  session: SessionRow | undefined;
  current: { providerId: string; model: string } | null;
  agent: AgentTemplate | null;
  compactTools: boolean;
  composerText: string;
  setComposerText: Dispatch<SetStateAction<string>>;
  pendingAttachments: AttachmentMeta[];
  setPendingAttachments: Dispatch<SetStateAction<AttachmentMeta[]>>;
  setPendingImages: Dispatch<SetStateAction<Record<string, string>>>;
  quotes: QuoteRef[];
  setQuotes: Dispatch<SetStateAction<QuoteRef[]>>;
  onPickFiles: (files: File[], notice?: string, rejected?: SkippedFile[]) => void;
  onRemoveAttachment: (index: number) => void;
  thinkingSel: ThinkingSelection | undefined;
  thinkCap: ReasoningCapability | null;
  sending: boolean;
  searching: boolean;
  onSend: () => void;
  onCancel: () => void;
  onRecall: () => void;
  onToggleWebSearch: () => void;
  setError: (msg: string) => void;
  /** 用户主动发送:恢复「跟随到底部」 */
  onResumeFollow: () => void;
  onOpenModelSwitch: () => void;
  onOpenAgentPicker: () => void;
  onSelectThinking: (sel: ThinkingSelection) => void;
}

/** 会话页输入区:输入框 + 工具条智能体指示 + 附件中心导入选择器。 */
export function ChatComposerArea({
  sessionId,
  session,
  current,
  agent,
  compactTools,
  composerText,
  setComposerText,
  pendingAttachments,
  setPendingAttachments,
  setPendingImages,
  quotes,
  setQuotes,
  onPickFiles,
  onRemoveAttachment,
  thinkingSel,
  thinkCap,
  sending,
  searching,
  onSend,
  onCancel,
  onRecall,
  onToggleWebSearch,
  setError,
  onResumeFollow,
  onOpenModelSwitch,
  onOpenAgentPicker,
  onSelectThinking,
}: ChatComposerAreaProps) {
  /** 从附件中心导入的选择器开关 */
  const [centerPickerOpen, setCenterPickerOpen] = useState(false);
  /**
   * 小窗紧凑态只保留「附件 + 模型」:联网、思考强度、智能体选择不渲染,
   * 对应默认值即联网关闭、思考关闭、智能体为空。
   */
  const showFullTools = !compactTools;

  /** 附件中心导入完成:追加待发区;图片补读 data URL 供视觉模型使用 */
  const handleCenterImported = useCallback(async (metas: AttachmentMeta[]) => {
    setPendingAttachments((prev) => [...prev, ...metas]);
    const imgs: Record<string, string> = {};
    for (const m of metas) {
      if (m.kind !== 'image') continue;
      try {
        imgs[m.path] = await ipc.readAttachmentData(m.path);
      } catch {
        // 忽略读取失败:图片仍以占位形式随消息发送
      }
    }
    if (Object.keys(imgs).length > 0) {
      setPendingImages((prev) => ({ ...prev, ...imgs }));
    }
  }, [setPendingAttachments, setPendingImages]);

  return (
    <>
      <Composer
        value={composerText}
        onChange={setComposerText}
        onSend={() => {
          // 用户主动发送:恢复「跟随到底部」
          onResumeFollow();
          onSend();
        }}
        disabled={sending || searching}
        variant="docked"
        sending={sending}
        onStop={onCancel}
        iconOnlyTools={compactTools}
        attachments={pendingAttachments}
        onPickFiles={onPickFiles}
        onRemoveAttachment={onRemoveAttachment}
        onOpenAttachmentCenter={() => setCenterPickerOpen(true)}
        quotes={quotes}
        onRemoveQuote={(qid) => setQuotes((prev) => prev.filter((q) => q.id !== qid))}
        onRecall={onRecall}
        webSearch={showFullTools ? (session?.web_search_enabled ?? false) : undefined}
        onToggleWebSearch={showFullTools ? onToggleWebSearch : undefined}
        thinking={
          showFullTools && thinkCap?.enabled && current
            ? {
                capability: thinkCap,
                providerId: current.providerId,
                model: current.model,
                selection: thinkingSel ?? { on: false, level: DEFAULT_THINKING_LEVEL },
                onSelect: onSelectThinking,
              }
            : null
        }
        model={{
          name: current ? current.model : '未配置模型',
          ready: !!current,
          onOpen: onOpenModelSwitch,
        }}
      >
        {/* B2:当前 Agent 指示,点击切换/解绑;小窗紧凑态不展示 */}
        {showFullTools && (
          <button
            type="button"
            className="tool-chip agent-chip-btn"
            title={agent ? `当前智能体:${agent.name}(点击切换)` : '点击选择智能体'}
            onClick={onOpenAgentPicker}
          >
            {(() => {
              const Icon = agentIconComponent(agent?.icon ?? 'bot');
              return <Icon size={13} />;
            })()}
            {!compactTools && <span className="agent-chip-text">{agent?.name ?? '智能体'}</span>}
          </button>
        )}
      </Composer>
      {centerPickerOpen && (
        <AttachmentCenterPicker
          sessionId={sessionId}
          onClose={() => setCenterPickerOpen(false)}
          onImported={(metas) => void handleCenterImported(metas)}
          onError={setError}
        />
      )}
    </>
  );
}
