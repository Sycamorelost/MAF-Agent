//! 设置 · 运行时预设视图(P5 / D18 / D19):全局默认档选择 + 四档能力对照(只读)。
//! 预设与权限解耦:这里只决定工具/上下文/检索的默认档,审批档位仍在「MCP 工具」里管。

import { useEffect, useState } from 'react';
import { IconCheck, IconGauge } from '../../components/icons';
import { listPresets, type Preset } from '../../lib/ipc/presets';
import {
  approvalSummary,
  contextSummary,
  memorySummary,
  retrievalSummary,
  toolsSummary,
} from '../../lib/presets/resolve';

export interface PresetsViewProps {
  /** 当前全局默认档 id;null = 设置尚未加载 */
  current: string | null;
  onSelect: (presetId: string) => void;
}

export function PresetsView({ current, onSelect }: PresetsViewProps) {
  const [presets, setPresets] = useState<Preset[]>([]);

  useEffect(() => {
    listPresets().then(setPresets).catch(() => {});
  }, []);

  return (
    <>
      <h2>运行时预设</h2>
      <p className="set-desc">
        预设决定「给模型多少工具、多少上下文、怎么检索」;与布局(工作台模式)和权限(审批档)互不干扰。
        新建会话按此处默认档钉住,已有会话可在工作台顶栏的预设 chip 单独切换。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          <IconGauge size={14} />
          选择默认档
          <span className="st">自动保存</span>
        </div>
        <div className="set-card-b">
          <div className="preset-grid">
            {presets.map((p) => {
              const on = current === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  className={`preset-card${on ? ' on' : ''}`}
                  onClick={() => onSelect(p.id)}
                >
                  <span className="preset-card-h">
                    {p.name}档
                    <span className="sp" />
                    {on && <IconCheck size={12} />}
                  </span>
                  <span className="preset-card-d">{p.description}</span>
                  <span className="preset-card-m">
                    <span>{toolsSummary(p)}</span>
                    <span>{contextSummary(p)}</span>
                    <span>{retrievalSummary(p)}</span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">四档能力对照</div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <table className="matrix">
            <thead>
              <tr>
                <th style={{ width: 88 }}>维度</th>
                {presets.map((p) => (
                  <th key={p.id} className={current === p.id ? 'on' : ''}>
                    {p.name}档
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {(
                [
                  ['工具集', toolsSummary],
                  ['上下文', contextSummary],
                  ['记忆注入', memorySummary],
                  ['审批默认', approvalSummary],
                  ['检索策略', retrievalSummary],
                ] as [string, (p: Preset) => string][]
              ).map(([label, fn]) => (
                <tr key={label}>
                  <td className="k">{label}</td>
                  {presets.map((p) => (
                    <td key={p.id} className={current === p.id ? 'on' : ''}>
                      {fn(p)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="set-note">
        <IconGauge size={14} />
        <span>
          <b>三级生效:</b>
          会话钉住 &gt; Agent 模板默认(代码类模板默认「代码」档)&gt; 此处全局默认;
          「审批默认」只是建议值,实际审批始终以会话权限档(逐次 / 工作区 / 完全信任)为准。
        </span>
      </div>
    </>
  );
}
