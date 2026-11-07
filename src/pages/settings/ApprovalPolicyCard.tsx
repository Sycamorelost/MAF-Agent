import { IconCheck } from '../../components/icons';
import type { ApprovalMode, ApprovalPolicy } from '../../lib/ipc/mcp';

/** 审批档选项(逐次 / 工作区 / 完全信任),默认「逐次审批」。 */
export const APPROVAL_MODES: { id: ApprovalMode; title: string; desc: string }[] = [
  { id: "ask", title: "逐次审批", desc: "每个工具调用都弹窗确认,最安全(默认)" },
  {
    id: "workspace",
    title: "工作区",
    desc: "只读与工作区内写入自动放行,shell / 删除 / 越界写入仍询问",
  },
  { id: "full", title: "完全信任", desc: "任何工具调用都不再弹窗(含高危 shell、工作区外读写),GUI 自动化可用" },
];

/** 审批档卡片:全局永久设置,三档选择 + 工作区附加白名单。 */
export function ApprovalPolicyCard({
  policy,
  allowText,
  onAllowTextChange,
  onPickMode,
  onSaveAllowList,
}: {
  policy: ApprovalPolicy;
  allowText: string;
  onAllowTextChange: (v: string) => void;
  onPickMode: (mode: ApprovalMode) => void;
  onSaveAllowList: () => void;
}) {
  return (
    <div className="mcp-policy-card">
      <div className="mcp-time-label">工具权限(全局永久生效)</div>
      <div className="mcp-policy-desc">
        此设置对所有会话、所有环境统一生效,保存后重启应用仍然有效。
        默认「逐次审批」:每次工具调用都要你确认;「工作区」:只读与工作区内写入免确认;
        「完全信任」:包括高危 shell 命令与工作区外读写在内的所有调用都不再确认,并解锁 GUI 自动化。
      </div>
      <div className="mcp-modes">
        {APPROVAL_MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            className={`mcp-mode${policy.mode === m.id ? " on" : ""}`}
            onClick={() => onPickMode(m.id)}
          >
            <span className="mcp-mode-body">
              <span className="mcp-mode-title">{m.title}</span>
              <span className="mcp-mode-desc">{m.desc}</span>
            </span>
            {policy.mode === m.id && (
              <span className="mcp-mode-check">
                <IconCheck size={12} />
              </span>
            )}
          </button>
        ))}
      </div>
      {policy.mode === "full" && (
        <div className="mcp-warn">
          完全信任已开启:所有工具调用(含高危 shell、删除、工作区外读写)都不再询问,模型可执行任意命令与系统环境操作。
          请仅在同时信任所用模型与全部工具输出内容时使用。
        </div>
      )}
      {policy.mode === "workspace" && (
        <label className="mcp-field mcp-allow-field">
          <span>附加自动放行的工具(每行一条,如 项目文件::read_text_file 或 read_text_file)</span>
          <textarea
            className="mcp-allow-input"
            rows={4}
            value={allowText}
            onChange={(e) => onAllowTextChange(e.target.value)}
            placeholder={"项目文件::read_text_file\n长期记忆::search_nodes"}
          />
          <button className="btn btn-xs" onClick={onSaveAllowList}>
            保存白名单
          </button>
        </label>
      )}
    </div>
  );
}
