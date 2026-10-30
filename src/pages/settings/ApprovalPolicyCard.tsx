import { IconCheck } from '../../components/icons';
import type { ApprovalMode, ApprovalPolicy } from '../../lib/ipc/mcp';

/** 审批档选项(D11 三档收敛:逐次 / 工作区 / 完全信任)。 */
export const APPROVAL_MODES: { id: ApprovalMode; title: string; desc: string }[] = [
  { id: "ask", title: "逐次审批", desc: "每个工具调用都弹窗确认,最保守" },
  {
    id: "workspace",
    title: "工作区",
    desc: "只读与工作区内写入自动放行,shell / 删除 / 越界写入仍询问(默认)",
  },
  { id: "full", title: "完全信任", desc: "不再弹窗,任意命令直接运行(写删仍备份并审计)" },
];

/** 审批档卡片:三档选择、完全信任警示与工作区附加白名单(D11)。 */
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
      <div className="mcp-time-label">如何批准 Agent 的操作?</div>
      <div className="mcp-policy-desc">
        「逐次审批」每次工具调用都确认;「工作区」只读与工作区内写入免弹窗,shell /
        删除 / 越界写入仍询问;「完全信任」不再弹窗,shell 可执行任意命令(含环境变量 / 服务 /
        注册表等系统环境操作),并解锁 GUI 自动化与临时授权。
        旧版「手动 / 自动 / 自定义」已并入三档:手动→逐次,自动 / 自定义→工作区(白名单保留为下方附加放行)。
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
          已开启完全信任:工具调用不再询问,shell 可执行任意命令(含任意路径与系统环境操作),
          GUI 自动化已解锁。请仅在同时信任模型与全部工具输出(网页 / 文件内容)时使用。
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
