import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ApprovalRequest, ApprovalResult, ApprovalRisk } from "../lib/ipc/mcp";
import { mcpResolveApproval } from "../lib/ipc/mcp";

const RISK_COLOR: Record<ApprovalRisk, string> = {
  low: "#22c55e",
  medium: "#eab308",
  high: "#ef4444",
};

/** HITL 审批对话框:监听 `mcp-approval-request` 事件,4 种决策按钮。 */
export default function McpApprovalDialog() {
  const [req, setReq] = useState<ApprovalRequest | null>(null);

  useEffect(() => {
    const unlisten = listen<ApprovalRequest>(
      "mcp-approval-request",
      (e) => {
        setReq(e.payload);
        // 供工作台运行状态条(P4)感知「等待审批」阶段
        window.dispatchEvent(new CustomEvent("mcp-approval-pending", { detail: true }));
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!req) return null;

  const resolve = (result: ApprovalResult) => {
    mcpResolveApproval(req.approval_id, result).then(() => {
      setReq(null);
      window.dispatchEvent(new CustomEvent("mcp-approval-pending", { detail: false }));
    });
  };

  return (
    <div className="mcp-approval-mask" onClick={() => resolve({ decision: "deny" })}>
      <div className="mcp-approval" onClick={(e) => e.stopPropagation()}>
        <div className="mcp-approval-head">
          <span className="mcp-approval-title">工具调用审批</span>
          <span
            className="mcp-approval-risk"
            style={{ background: RISK_COLOR[req.risk] }}
          >
            {req.risk === "low" ? "低风险" : req.risk === "medium" ? "中风险" : "高风险"}
          </span>
        </div>
        <div className="mcp-approval-meta">
          <span className="mcp-approval-chip">{req.server_name}</span>
          <span>→</span>
          <span className="mcp-approval-chip bold">{req.tool_name}</span>
        </div>
        <div className="mcp-approval-reason">{req.reason}</div>
        <details className="mcp-approval-args">
          <summary>查看参数</summary>
          <pre>{JSON.stringify(req.arguments, null, 2)}</pre>
        </details>
        <div className="mcp-approval-actions">
          <button className="btn btn-ghost" onClick={() => resolve({ decision: "deny" })}>
            拒绝
          </button>
          <button
            className="btn btn-ghost"
            onClick={() => resolve({ decision: "allow_always" })}
            title="本次会话内后续调用该工具不再询问"
          >
            始终允许
          </button>
          <button className="btn btn-primary" onClick={() => resolve({ decision: "allow" })}>
            允许
          </button>
        </div>
      </div>
    </div>
  );
}
