import { useEffect, useState } from 'react';
import { IconX } from '../icons';

export const SOURCE_MODES = [
  { id: 'web', label: '全网' },
  { id: 'academic', label: '学术优先' },
  { id: 'whitelist', label: '白名单' },
];

export const LANG_MODES = [
  { id: 'auto', label: '自动' },
  { id: 'zh', label: '中文' },
  { id: 'en', label: 'English' },
];

/** 逗号分隔文本输入:本地暂存,失焦/回车时解析提交(避免输入途中被截断)。 */
export function CsvInput({
  value,
  placeholder,
  onCommit,
}: {
  value: string[];
  placeholder?: string;
  onCommit: (arr: string[]) => void;
}) {
  const [text, setText] = useState(value.join(', '));
  useEffect(() => setText(value.join(', ')), [value]);
  const commit = () =>
    onCommit(
      text
        .split(/[,,、\s]+/)
        .map((s) => s.trim())
        .filter(Boolean),
    );
  return (
    <input
      className="pg-field"
      value={text}
      placeholder={placeholder}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        }
      }}
    />
  );
}

/** 数字覆盖 chip:null = 跟随全局;点击切换自定义。 */
export function OvNum({
  label,
  value,
  def,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number | null;
  def: number;
  min: number;
  max: number;
  onChange: (v: number | null) => void;
}) {
  if (value === null) {
    return (
      <span className="inherit-tag" title="点击覆盖全局值" onClick={() => onChange(def)}>
        {label} · 跟随全局
      </span>
    );
  }
  return (
    <span className="inherit-tag custom">
      {label}
      <input
        type="number"
        className="ov-num"
        min={min}
        max={max}
        value={value}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => onChange(Math.max(min, Math.min(max, Number(e.target.value) || def)))}
      />
      <i className="x" title="恢复跟随全局" onClick={() => onChange(null)}>
        <IconX size={10} />
      </i>
    </span>
  );
}

/** 选择覆盖 chip:null = 跟随全局;点击循环选项,右键或 × 恢复。 */
export function OvSel({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: { id: string; label: string }[];
  onChange: (v: string | null) => void;
}) {
  if (value === null) {
    return (
      <span className="inherit-tag" title="点击覆盖全局值" onClick={() => onChange(options[0].id)}>
        {label} · 跟随全局
      </span>
    );
  }
  return (
    <span className="inherit-tag custom">
      {label}
      <select value={value} onClick={(e) => e.stopPropagation()} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
      <i className="x" title="恢复跟随全局" onClick={() => onChange(null)}>
        <IconX size={10} />
      </i>
    </span>
  );
}

/** 约束数字输入:null = 不限(占位提示)。 */
export function GcNum({
  label,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  value: number | null;
  placeholder: string;
  onChange: (v: number | null) => void;
}) {
  return (
    <div className="gc-item">
      <span className="l">{label}</span>
      <input
        className="pg-field num"
        type="number"
        min={0}
        placeholder={placeholder}
        value={value ?? ''}
        onChange={(e) => {
          const t = e.target.value.trim();
          onChange(t === '' ? null : Math.max(0, Number(t) || 0));
        }}
      />
    </div>
  );
}
