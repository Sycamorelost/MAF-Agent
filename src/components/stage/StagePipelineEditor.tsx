/**
 * 阶段流水线编辑器(专项研究,步骤 2)。
 *
 * 视觉/结构严格遵循 plan/07-specialized-research-pages.html p1 L397-495:
 *   - stage-card 收起/展开、增删、上移下移(箭头按钮,不用 HTML5 DnD)、启停
 *   - 参数覆盖 inherit-tag「跟随全局」占位态,点击切换自定义
 *   - 阶段内约束 gc-grid / 必覆盖角度 / 域名黑名单 / 查询语言 / tool-chip 开关
 *   - 「撰写」型阶段仅保留名称/目标/摘要提示覆盖(后端 run_synthesis 只消费这些)
 */
import * as ipc from '../../lib/ipc';
import { IconArrowDown, IconPlus } from '../icons';
import { StageCard } from './StageCard';
import { useStageDrag } from './useStageDrag';

interface Props {
  stages: ipc.StageConfig[];
  onChange: (stages: ipc.StageConfig[]) => void;
}

export function StagePipelineEditor({ stages, onChange }: Props) {
  const { expandedId, toggle, patchAt, removeAt, move, addStage } = useStageDrag({ stages, onChange });
  const enabled = stages.filter((s) => s.enabled).length;

  return (
    <div className="stage-pipe">
      <div className="pg-lab">
        阶段流水线
        <span className="sp" />
        <span className="n">
          {stages.length} 阶段 · {enabled} 启用
        </span>
      </div>
      {stages.map((st, i) => (
        <div key={st.id}>
          {i > 0 && (
            <div className="stage-conn">
              <IconArrowDown size={14} />
            </div>
          )}
          <StageCard
            st={st}
            idx={i}
            total={stages.length}
            expanded={expandedId === st.id}
            onToggle={() => toggle(st.id)}
            onPatch={(p) => patchAt(i, p)}
            onRemove={() => removeAt(i)}
            onMove={(d) => move(i, d)}
          />
        </div>
      ))}
      <div className="stage-add">
        <button className="btn-ghost" onClick={() => addStage('research')}>
          <IconPlus size={13} />
          检索阶段
        </button>
        <button className="btn-ghost" onClick={() => addStage('synthesis')}>
          <IconPlus size={13} />
          撰写阶段
        </button>
      </div>
      <div className="pg-hint">
        「撰写」型阶段不检索,仅基于已有资料产出分析文本;终稿在全部阶段后自动生成。
      </div>
    </div>
  );
}
