import { describe, expect, it } from 'vitest';
import { computeMenuPos } from './usePinMenu';

const VIEWPORT = { width: 1000, height: 800 };

describe('computeMenuPos', () => {
  it('上方空间充足时向上展开,底边贴在锚点上方 8px', () => {
    const pos = computeMenuPos(
      { top: 600, bottom: 628, left: 120 },
      VIEWPORT,
      { width: 176, minHeight: 120 },
    );
    expect(pos.placement).toBe('up');
    expect(pos.bottom).toBe(800 - 600 + 8);
    expect(pos.left).toBe(120);
    expect(pos.maxHeight).toBeGreaterThanOrEqual(120);
  });

  it('上方空间不足时向下翻转,顶边贴在锚点下方 8px', () => {
    const pos = computeMenuPos(
      { top: 90, bottom: 118, left: 40 },
      { width: 1000, height: 600 },
      { width: 176, minHeight: 120 },
    );
    expect(pos.placement).toBe('down');
    expect(pos.top).toBe(118 + 8);
    expect(pos.bottom).toBeUndefined();
  });

  it('左边缘超出窗口时向右钳制,至少保留 8px 边距', () => {
    const pos = computeMenuPos(
      { top: 600, bottom: 628, left: 980 },
      VIEWPORT,
      { width: 300, minHeight: 120 },
    );
    expect(pos.left).toBe(1000 - 300 - 8);
  });

  it('菜单宽于窗口时钳制到窗口内并保留左右边距', () => {
    const pos = computeMenuPos(
      { top: 600, bottom: 628, left: 0 },
      { width: 240, height: 600 },
      { width: 500, minHeight: 120 },
    );
    expect(pos.left).toBe(8);
    expect(pos.maxHeight).toBeLessThanOrEqual(600);
  });

  it('上下空间都极小时仍保留可用兜底高度', () => {
    const pos = computeMenuPos(
      { top: 60, bottom: 88, left: 20 },
      { width: 800, height: 180 },
      { width: 176, minHeight: 120 },
    );
    expect(pos.placement).toBe('down');
    expect(pos.maxHeight).toBeGreaterThanOrEqual(96);
  });
});
