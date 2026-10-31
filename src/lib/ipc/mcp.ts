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
  /** 启动子进程时注入的环境变量(如 TAVILY_API_KEY;预置联网搜索会自动注入已存 Key) */
  env?: Record<string, string>;
}

export interface McpConfig {
  servers: McpServerConfig[];
  /** Phase 6:是否向模型暴露内置工具(get_now / shell_exec / fs_*)。默认开启(开箱即用)。 */
  builtin_tools?: boolean;
  /** 对话工具调用总开关:关闭时不下发任何工具定义(省 token/降延迟)。默认关闭。 */
  tools_enabled?: boolean;
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

/** 工具审批档(D11 三档收敛):逐次审批 / 工作区(默认) / 完全信任。
 *  旧四档自动迁移:manual→ask,auto/custom→workspace(白名单保留为 workspace 附加放行)。 */
export type ApprovalMode = "ask" | "workspace" | "full";

export interface ApprovalPolicy {
  mode: ApprovalMode;
  /** workspace 档下附加自动放行的工具,支持 `server::tool` 或只写 `tool` */
  allow: string[];
}

// ------- IPC 函数 -------

export const mcpLoadConfig = () => invoke<McpConfig>("mcp_load_config");
export const mcpSaveConfig = (cfg: McpConfig) => invoke<void>("mcp_save_config", { cfg });

/** 清除「长期记忆」全局数据(data/mcp-memory.jsonl);先停 server 再删文件,返回删除的字节数。 */
export const mcpClearMemory = () => invoke<number>("mcp_clear_memory");
export const mcpAddServer = (server: McpServerConfig) => invoke<void>("mcp_add_server", { server });
export const mcpRemoveServer = (name: string) => invoke<void>("mcp_remove_server", { name });

/** 启动 server —— 参数取自已保存配置,stdio 与 http 共用。 */
export const mcpStartServer = (name: string) => invoke<void>("mcp_start_server", { name });

/** 一键启动所有 enabled 的 MCP server。返回 [成功数, 失败数, 错误信息列表]。 */
export const mcpStartAllEnabled = () =>
  invoke<[number, number, string[]]>("mcp_start_all_enabled");

export const mcpListTools = (name: string) =>
  invoke<McpToolInfo[]>("mcp_list_tools", { name });

export const mcpCallToolWithApproval = (call: McpToolCall) =>
  invoke<McpToolResult>("mcp_call_tool_with_approval", { call });

export const mcpResolveApproval = (approvalId: string, result: ApprovalResult) =>
  invoke<boolean>("mcp_resolve_approval", { approvalId, result });

export const mcpClearSessionAllowed = () => invoke<void>("mcp_clear_session_allowed");

/** 启动初始化状态(0=无/未开始,1=初始化中,2=完成);完成时后端广播 bootstrap_done 事件。 */
export const getBootstrapState = () => invoke<number>("get_bootstrap_state");

/** 读取 / 保存审批模式(保存后立即对下次工具调用生效)。 */
export const mcpGetApprovalPolicy = () => invoke<ApprovalPolicy>("mcp_get_approval_policy");
export const mcpSetApprovalPolicy = (policy: ApprovalPolicy) =>
  invoke<void>("mcp_set_approval_policy", { policy });

/** Phase 5e:汇总所有已启用 server 的工具定义(OpenAI function-calling)。
 *  P5/D18:传 sessionId 时后端按会话生效预设裁剪工具名单;不传则不过滤。 */
export const mcpCollectTools = (sessionId?: string) =>
  invoke<ToolDef[]>("mcp_collect_tools", { sessionId: sessionId ?? null });
