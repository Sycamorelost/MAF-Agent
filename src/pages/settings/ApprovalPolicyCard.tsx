import type { ApprovalMode, ApprovalPolicy } from '../../lib/ipc/mcp';

/** 审批模式选项(对齐「如何批准 Agent 的操作」四档)。 */
export const APPROVAL_MODES: { id: ApprovalMode; title: string; desc: string }[] = [
  { id: "manual", title: "手动审批", desc: "每次工具调用都由你确认" },
  { id: "auto", title: "自动审批", desc: "只读查询自动放行，写入 / 执行仍询问" },
  { id: "full", title: "完全信任", desc: "不再弹窗，任意命令直接运行（写删仍备份并审计）" },
  { id: "custom", title: "自定义", desc: "命中下方白名单的自动放行，其余询问" },
];

/** 审批模式卡片:四档模式选择、完全信任提示与自定义白名单。 */
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
        「手动审批」每次工具调用都确认;「自动审批」只读工具免弹窗;
        「完全信任」不再弹窗,shell 可执行任意命令(含环境变量 / 服务 / 注册表等系统环境操作),
        并解锁 GUI 自动化与临时授权。文件读写默认即全盘可用,不随模式变化。
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
            {policy.mode === m.id && <span className="mcp-mode-check">✓</span>}
          </button>
        ))}
      </div>
      {policy.mode === "full" && (
        <div className="mcp-warn">
          已开启完全信任:工具调用不再询问,shell 可执行任意命令(含任意路径与系统环境操作),
          GUI 自动化已解锁。请仅在同时信任模型与全部工具输出(网页 / 文件内容)时使用。
        </div>
      )}
      {policy.mode === "custom" && (
        <label className="mcp-field mcp-allow-field">
          <span>自动放行的工具(每行一条,如 项目文件::read_text_file 或 read_text_file)</span>
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
