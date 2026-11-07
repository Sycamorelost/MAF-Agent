/**
 * 共享线性 SVG 图标库。
 *
 * 规则(见 project_memory.md):
 *   - stroke 风格,1.5px 描边,round caps & joins
 *   - 默认 18px,可通过 size 覆盖
 *   - 颜色继承 currentColor,由父级控制
 *   - 禁止在 UI 中使用 emoji;所有图标必须走这里
 */

export interface IconProps {
  size?: number;
  className?: string;
  strokeWidth?: number;
}

export function I({
  size = 18,
  className,
  strokeWidth = 1.5,
  children,
  viewBox = '0 0 24 24',
}: IconProps & { children: React.ReactNode; viewBox?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}
