import type { PanelResize } from '../lib/usePanelWidth';

/** 侧栏宽度拖拽把手:悬停/拖拽时高亮品牌色。 */
export function ResizeHandle({ panel }: { panel: PanelResize }) {
  return (
    <div
      className={`resize-handle${panel.dragging ? ' dragging' : ''}`}
      title="拖动调整宽度"
      onPointerDown={panel.onHandleDown}
      onPointerMove={panel.onHandleMove}
      onPointerUp={panel.onHandleUp}
    />
  );
}
