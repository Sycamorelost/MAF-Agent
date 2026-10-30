//! 工作台首页「新建任务」输入卡(ui-design b2):多行目标 + 档位/权限 chip + 附件/记忆入口 + 主行动。
//! 从 WorkbenchHome 拆出,保持首页壳文件精简(val.md §5.3)。

import { IconAttach, IconBrain, IconGauge, IconPlus, IconShield } from '../../../components/icons';

export function NewTaskBox({
  text,
  presetName,
  creating,
  onText,
  onCreate,
  onOpenPreset,
}: {
  text: string;
  presetName: string;
  creating: boolean;
  onText: (v: string) => void;
  onCreate: () => void;
  onOpenPreset: () => void;
}) {
  return (
    <div className="newtask">
      <textarea
        className="newtask-input"
        rows={2}
        placeholder="描述任务目标,例如:把 dev_plan/val.md 的四档预设整理成一页 PDF,并给出档位选择建议。"
        value={text}
        onChange={(e) => onText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            onCreate();
          }
        }}
      />
      <div className="newtask-bar">
        <button
          type="button"
          className="chip"
          title="当前档位(新任务继承全局默认,可在会话内切换)"
          onClick={onOpenPreset}
        >
          <IconGauge size={12} />
          {presetName}
        </button>
        <span className="chip" title="新会话的默认写入范围(可在会话内切换)">
          <IconShield size={12} />
          工作区写入
        </span>
        <button type="button" className="icon-btn" title="附件:新建任务后可在会话内添加" disabled>
          <IconAttach size={14} />
        </button>
        <button type="button" className="icon-btn" title="记忆:新建任务后可在会话内开关" disabled>
          <IconBrain size={14} />
        </button>
        <span className="sp" />
        <button
          type="button"
          className="btn-primary"
          disabled={creating || !text.trim()}
          onClick={onCreate}
        >
          <IconPlus size={14} />
          {creating ? '创建中…' : '新建任务'}
        </button>
      </div>
    </div>
  );
}
