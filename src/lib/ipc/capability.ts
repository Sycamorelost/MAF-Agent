//! 能力令牌 IPC —— 对应 Rust 侧 commands/capability.rs。

import { invoke } from "@tauri-apps/api/core";

/** 令牌作用域(与 Rust `Scope` 的 snake_case 序列化一致)。 */
export type Scope = "fs_read" | "fs_write" | "exec" | "net" | "ui";

export interface Capability {
  id: string;
  label: string;
  scopes: Scope[];
  /** 签发时间(unix 秒) */
  issued_at: number;
  /** 过期时间(unix 秒) */
  expires_at: number;
  /** 最大使用次数;null = 不限 */
  max_uses: number | null;
  uses: number;
}

export const SCOPE_META: { id: Scope; label: string; desc: string }[] = [
  { id: "fs_read", label: "文件只读", desc: "fs_list / fs_read / 解压查看" },
  { id: "fs_write", label: "文件写入", desc: "fs_write / fs_delete / 导出" },
  { id: "exec", label: "命令执行", desc: "shell_exec / 外部写操作" },
  { id: "net", label: "MCP 查询", desc: "外部 MCP 只读工具" },
  { id: "ui", label: "GUI 自动化", desc: "点击 / 写值 / 输入 / 聚焦" },
];

export const scopeLabel = (s: Scope) => SCOPE_META.find((m) => m.id === s)?.label ?? s;

/** 签发令牌(ttlSecs 省略 = 30 分钟;maxUses 省略 = 不限次数)。 */
export const mcpIssueCapability = (
  label: string,
  scopes: Scope[],
  ttlSecs?: number,
  maxUses?: number,
) =>
  invoke<Capability>("mcp_issue_capability", {
    label,
    scopes,
    ttlSecs: ttlSecs ?? null,
    maxUses: maxUses ?? null,
  });

/** 列出仍有效的令牌(过期项由后端自动剔除)。 */
export const mcpListCapabilities = () => invoke<Capability[]>("mcp_list_capabilities");

export const mcpRevokeCapability = (id: string) =>
  invoke<boolean>("mcp_revoke_capability", { id });

/** 清空全部令牌(重启应用也会清空,因为只在内存中)。 */
export const mcpClearCapabilities = () => invoke<void>("mcp_clear_capabilities");
