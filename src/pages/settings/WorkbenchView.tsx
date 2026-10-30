//! 设置 · 工作台模式视图(P3 / D7 / D16):全局默认模式选择,点击即自动保存。

import { IconChat, IconCheck, IconLayers, IconSparkles } from '../../components/icons';

export interface WorkbenchViewProps {
  /** 当前全局默认(chat / work);null = 设置尚未加载 */
  current: 'chat' | 'work' | null;
  onSelect: (mode: 'chat' | 'work') => void;
}

interface ChoiceDef {
  mode: 'chat' | 'work';
  title: string;
  desc: string;
  feats: string[];
  foot: string;
}

const CHOICES: ChoiceDef[] = [
  {
    mode: 'work',
    title: '专业 Agent',
    desc: '三栏工作台:计划、对话与检查器同时可见,适合多步任务与交付物。',
    feats: [
      '任务计划:待办 / 进行中 / 已完成',
      '检查器四页签:产物、变更、上下文、流水',
      '子任务卡片、回滚与一键压缩',
    ],
    foot: '新建会话默认进入专业首页',
  },
  {
    mode: 'chat',
    title: '普通会话',
    desc: '单栏对话与输入框,隐藏任务计划与检查器,适合快速问答与写作。',
    feats: ['首页即对话,路径最短', '不显示运行状态与产物列表', '仍可调用工具,但不展开过程'],
    foot: '新建会话直接进入输入框',
  },
];

const MATRIX: [string, string, string][] = [
  ['首页', '欢迎语 + 输入框', '专业首页:模式徽标、新建任务、最近任务'],
  ['布局', '单栏对话', '左会话 / 中对话 / 右检查器'],
  ['任务计划', '不展示', '待办、进行中、已完成与进度'],
  ['产物与回滚', '仅以附件形式出现', '产物页签 + diff + 两种回滚粒度'],
  ['适用', '问答、翻译、轻写作', '多步检索、文档交付、代码改动'],
];

export function WorkbenchView({ current, onSelect }: WorkbenchViewProps) {
  return (
    <>
      <h2>工作台模式</h2>
      <p className="set-desc">
        决定首页默认落点,以及新建会话是单栏对话还是三栏工作台。切换默认值不会改变已有会话。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          <IconLayers size={14} />
          选择默认模式
          <span className="st">自动保存</span>
        </div>
        <div className="set-card-b">
          <div className="choice-grid">
            {CHOICES.map((c) => {
              const on = current === c.mode;
              return (
                <button
                  key={c.mode}
                  type="button"
                  className={`choice${on ? ' on' : ''}`}
                  onClick={() => onSelect(c.mode)}
                >
                  <span className="choice-mark">
                    <IconCheck size={12} />
                  </span>
                  <span className="choice-ic">
                    {c.mode === 'work' ? <IconLayers size={20} /> : <IconChat size={20} />}
                  </span>
                  <h3>{c.title}</h3>
                  <p>{c.desc}</p>
                  <ul>
                    {c.feats.map((f) => (
                      <li key={f}>
                        <IconCheck size={13} />
                        {f}
                      </li>
                    ))}
                  </ul>
                  <div className="choice-foot">
                    <IconSparkles size={12} />
                    {c.foot}
                    <span className="sp" />
                    {on && <span className="tag-cur">当前默认</span>}
                  </div>
                </button>
              );
            })}
          </div>
          <div className="set-note">
            <IconSparkles size={14} />
            <span>
              <b>单会话可覆盖:</b>
              会话头部的「切到普通对话 / 切到工作台」只改变当前会话布局,不改变此处的默认值;已运行的任务在普通视图下保留摘要入口。
            </span>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">两者差异</div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <table className="matrix">
            <thead>
              <tr>
                <th style={{ width: 120 }}>维度</th>
                <th className={current === 'chat' ? 'on' : ''}>普通会话</th>
                <th className={current === 'work' ? 'on' : ''}>专业 Agent</th>
              </tr>
            </thead>
            <tbody>
              {MATRIX.map(([k, chat, work]) => (
                <tr key={k}>
                  <td className="k">{k}</td>
                  <td className={current === 'chat' ? 'on' : ''}>{chat}</td>
                  <td className={current === 'work' ? 'on' : ''}>{work}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
