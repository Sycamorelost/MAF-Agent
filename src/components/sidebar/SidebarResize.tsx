import { useRef } from 'react';
import { clampSidebarWidth, defaultSidebarWidth } from './utils';

interface Props {
  visible: boolean;
  width: number;
  onWidth: (w: number) => void;
  onDraggingChange: (dragging: boolean) => void;
}

/** 右侧拖拽调宽手柄:拖拽中临时关闭抽屉宽度过渡(保证跟手),双击复位默认宽度。 */
export function SidebarResize({ visible, width, onWidth, onDraggingChange }: Props) {
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!visible) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { startX: e.clientX, startW: width };
    onDraggingChange(true);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d) return;
    onWidth(clampSidebarWidth(d.startW + e.clientX - d.startX));
  };
  const onPointerEnd = () => {
    dragRef.current = null;
    onDraggingChange(false);
  };

  return (
    <div
      className="side-resize"
      role="separator"
      aria-orientation="vertical"
      aria-label="拖拽调整侧栏宽度"
      title="拖拽调整宽度,双击复位"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onDoubleClick={() => onWidth(defaultSidebarWidth())}
    />
  );
}
