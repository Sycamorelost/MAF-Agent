import * as ipc from '../../../lib/ipc';

/** 六维雷达(SVG 六边形)。 */
export function Radar({ dims }: { dims: ipc.AiflavorDim[] }) {
  if (dims.length !== 6) return null;
  const CX = 70;
  const CY = 64;
  const R = 46;
  const pt = (i: number, v: number) => {
    const a = (-90 + i * 60) * (Math.PI / 180);
    const r = (v / 100) * R;
    return `${CX + r * Math.cos(a)},${CY + r * Math.sin(a)}`;
  };
  const poly = dims.map((d, i) => pt(i, d.score)).join(' ');
  const LABELS = ['套话', '句长', '词汇', '连接', '具体', '模板'];
  return (
    <svg width="140" height="128" viewBox="0 0 140 128" className="aif-radar">
      {[0.25, 0.5, 0.75, 1].map((k) => (
        <polygon
          key={k}
          points={dims.map((_, i) => pt(i, k * 100)).join(' ')}
          fill="none"
          stroke="var(--border)"
          strokeWidth="0.6"
        />
      ))}
      {dims.map((_, i) => (
        <line
          key={i}
          x1={CX}
          y1={CY}
          x2={pt(i, 100).split(',')[0]}
          y2={pt(i, 100).split(',')[1]}
          stroke="var(--border)"
          strokeWidth="0.6"
        />
      ))}
      <polygon points={poly} fill="rgba(201,59,59,.25)" stroke="var(--err)" strokeWidth="1.2" />
      {LABELS.map((l, i) => {
        const [x, y] = pt(i, 118).split(',');
        return (
          <text key={l} x={x} y={y} textAnchor="middle" dominantBaseline="middle" className="aif-radar-lbl">
            {l}
          </text>
        );
      })}
    </svg>
  );
}
