import { createPortal } from 'react-dom';
import type { ChatSelectionTool } from './useChatSelectionToolbar';

interface ChatSelectionToolbarProps {
  selection: ChatSelectionTool;
  onClose: () => void;
  /** 引用并追问 */
  onQuote: (selection: ChatSelectionTool) => void;
}

/** 划词选中浮动工具栏 —— Portal 到 body,绝对定位在选区上方。 */
export function ChatSelectionToolbar({ selection, onClose, onQuote }: ChatSelectionToolbarProps) {
  return createPortal(
    <div
      className="sel-float-tool"
      style={{ left: selection.x, top: selection.y }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button
        className="sel-ft-btn"
        onClick={() => {
          void navigator.clipboard.writeText(selection.text).catch(() => {});
          onClose();
        }}
        title="复制"
      >复制</button>
      <button
        className="sel-ft-btn primary"
        onClick={() => onQuote(selection)}
        title="引用并追问"
      >追问</button>
    </div>,
    document.body,
  );
}
