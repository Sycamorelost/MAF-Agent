import { useState } from 'react';
import * as ipc from '../../lib/ipc';

/** 阶段流水线的展开态与排序/增删/改参操作(上移下移按钮排序,非 HTML5 DnD)。 */
export function useStageDrag({
  stages,
  onChange,
}: {
  stages: ipc.StageConfig[];
  onChange: (stages: ipc.StageConfig[]) => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const toggle = (id: string) => setExpandedId(expandedId === id ? null : id);
  const patchAt = (i: number, p: Partial<ipc.StageConfig>) =>
    onChange(stages.map((s, j) => (j === i ? { ...s, ...p } : s)));
  const removeAt = (i: number) => onChange(stages.filter((_, j) => j !== i));
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= stages.length) return;
    const next = [...stages];
    [next[i], next[j]] = [next[j], next[i]];
    onChange(next);
  };
  const addStage = (kind: string) => {
    const st = ipc.newStage('', kind);
    onChange([...stages, st]);
    setExpandedId(st.id);
  };

  return { expandedId, toggle, patchAt, removeAt, move, addStage };
}
