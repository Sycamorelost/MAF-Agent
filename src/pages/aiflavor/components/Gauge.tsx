/** 总分仪表(SVG 圆环)。 */
export function Gauge({ score }: { score: number }) {
  const R = 44;
  const C = 2 * Math.PI * R;
  const frac = Math.min(100, Math.max(0, score)) / 100;
  const color =
    score >= 90 ? '#c0392b' : score >= 75 ? 'var(--err)' : score >= 55 ? '#e67e22' : score >= 30 ? 'var(--warn)' : 'var(--ok)';
  return (
    <svg width="110" height="110" viewBox="0 0 110 110" className="aif-gauge">
      <circle cx="55" cy="55" r={R} fill="none" stroke="var(--border)" strokeWidth="9" />
      <circle
        cx="55"
        cy="55"
        r={R}
        fill="none"
        stroke={color}
        strokeWidth="9"
        strokeLinecap="round"
        strokeDasharray={`${C * frac} ${C}`}
        transform="rotate(-90 55 55)"
      />
      <text x="55" y="52" textAnchor="middle" className="aif-gauge-num">
        {score}
      </text>
      <text x="55" y="70" textAnchor="middle" className="aif-gauge-sub">
        AI 味指数
      </text>
    </svg>
  );
}
