import MemoryHandoffDialog from '../../components/MemoryHandoffDialog';
import ModelSwitchDialog from '../../components/ModelSwitchDialog';
import AgentPickerDialog from '../../components/AgentPickerDialog';
import Toast from '../../components/Toast';
import type { BubbleMessage } from '../../components/MessageBubble';
import type { MemoryStrategy } from '../../lib/ipc';
import type { SwitchBanner } from './useModelHandoff';

interface ChatDialogsProps {
  sessionId: string;
  sessionAgentId: string | null;
  memoryBody: string;
  modelSwitchOpen: boolean;
  onCloseModelSwitch: () => void;
  onPickModel: (providerId: string, model: string) => void;
  /** F5:换模型重试的目标消息 */
  retryTarget: BubbleMessage | null;
  onCloseRetryModel: () => void;
  onPickRetryModel: (providerId: string, model: string) => void;
  agentPickerOpen: boolean;
  onCloseAgentPicker: () => void;
  onSelectAgent: (agentId: string | null) => Promise<void>;
  handoff: { from: string; to: string } | null;
  handoffDefaultStrategy: MemoryStrategy;
  onHandoffConfirm: (strategy: MemoryStrategy) => void;
  onCancelHandoff: () => void;
  showInjected: boolean;
  switchBanner: SwitchBanner | null;
  onCloseInjected: () => void;
  toast: string | null;
  onCloseToast: () => void;
}

/** 会话页弹窗集合:模型切换 / 换模型重试 / 智能体选择 / 记忆交接 / 注入内容预览 / 轻提示。 */
export function ChatDialogs({
  sessionId,
  sessionAgentId,
  memoryBody,
  modelSwitchOpen,
  onCloseModelSwitch,
  onPickModel,
  retryTarget,
  onCloseRetryModel,
  onPickRetryModel,
  agentPickerOpen,
  onCloseAgentPicker,
  onSelectAgent,
  handoff,
  handoffDefaultStrategy,
  onHandoffConfirm,
  onCancelHandoff,
  showInjected,
  switchBanner,
  onCloseInjected,
  toast,
  onCloseToast,
}: ChatDialogsProps) {
  return (
    <>
      {modelSwitchOpen && (
        <ModelSwitchDialog onClose={onCloseModelSwitch} onPick={onPickModel} />
      )}

      {/* F5:换模型重试(取消无副作用) */}
      {retryTarget && (
        <ModelSwitchDialog onClose={onCloseRetryModel} onPick={onPickRetryModel} />
      )}

      {agentPickerOpen && (
        <AgentPickerDialog
          currentAgentId={sessionAgentId}
          sessionId={sessionId}
          onSelect={onSelectAgent}
          onClose={onCloseAgentPicker}
        />
      )}

      {handoff && (
        <MemoryHandoffDialog
          from={handoff.from}
          to={handoff.to}
          memoryBody={memoryBody}
          currentStrategy={handoffDefaultStrategy}
          onConfirm={onHandoffConfirm}
          onCancel={onCancelHandoff}
        />
      )}

      {showInjected && switchBanner && (
        <div className="dialog-mask" onClick={onCloseInjected}>
          <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ width: 560 }}>
            <div className="dialog-head">实际注入的 system 文本</div>
            <div className="dialog-body">
              <pre className="injected-view">{switchBanner.injected}</pre>
            </div>
          </div>
        </div>
      )}

      {toast && <Toast text={toast} onClose={onCloseToast} />}
    </>
  );
}
