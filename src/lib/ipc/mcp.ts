//! MCP 工具相关 IPC 封装 —— 对应 Rust 侧 commands::mcp.rs。

import { invoke } from "@tauri-apps/api/core";
import type { ToolDef } from "./provider";

// ------- 类型定义(与 Rust 侧 serde 结构体镜像) -------

export type ApprovalRisk = "low" | "medium" | "high";

/** 传输方式:stdio 本地子进程 / http 远端 Streamable HTTP。 */
export type McpTransport = "stdio" | "http";

export interface McpServerConfig {
  name: string;
  enabled: boolean;
  transport: McpTransport;
  /** stdio:启动命令(如 npx / node) */
  command: string;
  args: string[];
  /** http:远端端点(https,或本机回环的 http) */
  url?: string;
  /** http:附加请求头,如 { Authorization: "Bearer …" } */
  headers?: Record<string, string>;
  allowed_dirs: string[];
  deny_patterns: string[];
}

export interface McpConfig {
  servers: McpServerConfig[];
  /** Phase 6:是否向模型暴露内置工具(get_now / shell_exec / fs_*)。默认开启(开箱即用)。 */
  builtin_tools?: boolean;
  /** 预置服务已合并到哪一版(后端用于补齐新增预置项,前端只做透传)。 */
  presets_version?: number;
  /** 下载资源优先使用国内镜像(npx→npmmirror,uvx/pip→清华 PyPI)。默认开启。 */
  china_mirror?: boolean;
}

export interface McpToolInfo {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

export interface McpToolCall {
  server_name: string;
  tool_name: string;
  arguments: Record<string, unknown>;
}

export interface McpToolResult {
  ok: boolean;
  content: string;
  error?: string;
}

export interface ApprovalRequest {
  approval_id: string;
  server_name: string;
  tool_name: string;
  arguments: Record<string, unknown>;
  risk: ApprovalRisk;
  reason: string;
}

export type ApprovalResult =
  | { decision: "allow" }
  | { decision: "allow_always" }
  | { decision: "allow_edited"; arguments: Record<string, unknown> }
  | { decision: "deny" };

/** 工具审批模式:手动 / 只读自动 / 完全放行 / 自定义白名单。 */
export type ApprovalMode = "manual" | "auto" | "full" | "custom";

export interface ApprovalPolicy {
  mode: ApprovalMode;
  /** 自定义模式下自动放行的工具,支持 `server::tool` 或只写 `tool` */
  allow: string[];
}

export interface NowResult {
  iso: string;
  unix_ms: number;
  tz: string;
  date: string;
  time: string;
  weekday: string;
}

// ------- IPC 函数 -------

export const mcpLoadConfig = () => invoke<McpConfig>("mcp_load_config");
export const mcpSaveConfig = (cfg: McpConfig) => invoke<void>("mcp_save_config", { cfg });
export const mcpAddServer = (server: McpServerConfig) => invoke<void>("mcp_add_server", { server });
export const mcpRemoveServer = (name: string) => invoke<void>("mcp_remove_server", { name });

/** 启动 server —— 参数取自已保存配置,stdio 与 http 共用。 */
export const mcpStartServer = (name: string) => invoke<void>("mcp_start_server", { name });

export const mcpListTools = (name: string) =>
  invoke<McpToolInfo[]>("mcp_list_tools", { name });

export const mcpCallToolWithApproval = (call: McpToolCall) =>
  invoke<McpToolResult>("mcp_call_tool_with_approval", { call });

export const mcpResolveApproval = (approvalId: string, result: ApprovalResult) =>
  invoke<boolean>("mcp_resolve_approval", { approvalId, result });

export const mcpClearSessionAllowed = () => invoke<void>("mcp_clear_session_allowed");

/** 读取 / 保存审批模式(保存后立即对下次工具调用生效)。 */
export const mcpGetApprovalPolicy = () => invoke<ApprovalPolicy>("mcp_get_approval_policy");
export const mcpSetApprovalPolicy = (policy: ApprovalPolicy) =>
  invoke<void>("mcp_set_approval_policy", { policy });

/** Phase 5e:汇总所有已启用 server 的工具定义(OpenAI function-calling)。 */
export const mcpCollectTools = () => invoke<ToolDef[]>("mcp_collect_tools");

export const getNow = () => invoke<NowResult>("get_now");
