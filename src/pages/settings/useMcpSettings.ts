﻿﻿﻿﻿﻿﻿﻿﻿﻿import { useEffect, useState } from 'react';
import {
  mcpLoadConfig,
  mcpSaveConfig,
  mcpAddServer,
  mcpGetApprovalPolicy,
  mcpSetApprovalPolicy,
  mcpDoctor,
  type ApprovalMode,
  type ApprovalPolicy,
  type McpConfig,
  type McpServerConfig,
} from '../../lib/ipc/mcp';
import * as ipc from '../../lib/ipc';
import {
  fsBackupList,
  fsOpenBackups,
  fsOpenWorkspace,
  fsPickWorkspaceDir,
  fsWorkspaceInfo,
} from '../../lib/ipc/fs';
import { APPROVAL_MODES } from './ApprovalPolicyCard';
import type { McpServerStatus } from './McpServerCard';
import { useMcpServerActions } from './useMcpServerActions';

/** MCP 设置页状态与保存动作(页面只负责展示与 props 组装)。 */
export function useMcpSettings() {
  const [cfg, setCfg] = useState<McpConfig>({ servers: [] });
  const [status, setStatus] = useState<Record<string, McpServerStatus>>({});
  const [loading, setLoading] = useState(true);
  const [msg, setMsg] = useState<string>("");
  const [showAdd, setShowAdd] = useState(false);
  const [policy, setPolicy] = useState<ApprovalPolicy>({ mode: "ask", allow: [] });
  const [allowText, setAllowText] = useState("");
  /** 文件工作区根目录与备份数量(内置 fs 工具的输出位置) */
  const [wsRoot, setWsRoot] = useState("");
  const [backupCount, setBackupCount] = useState(0);
  /** 应用设置(工作区根目录等,读取+单字段保存) */
  const [settings, setSettings] = useState<ipc.AppSettings | null>(null);

  useEffect(() => {
    ipc.getSettings().then(setSettings).catch(() => {});
  }, []);

  useEffect(() => {
    fsWorkspaceInfo()
      .then((i) => setWsRoot(i.root.replace(/^\\\\\?\\/, "")))
      .catch(() => {});
    fsBackupList()
      .then((b) => setBackupCount(b.length))
      .catch(() => {});
  }, []);

  useEffect(() => {
    mcpLoadConfig()
      .then(setCfg)
      .catch((e) => setMsg(`加载失败: ${e}`))
      .finally(() => setLoading(false));
    mcpGetApprovalPolicy()
      .then((p) => {
        setPolicy(p);
        setAllowText(p.allow.join("\n"));
      })
      .catch(() => {});
  }, []);

  /** 操作反馈消息 3 秒后自动消失。 */
  useEffect(() => {
    if (!msg) return;
    const timer = setTimeout(() => setMsg(""), 3000);
    return () => clearTimeout(timer);
  }, [msg]);

  /** 切换国内镜像(下载源):写入配置,已运行的 server 重启后生效。 */
  const onToggleMirror = async () => {
    if (!cfg) return;
    const next = !(cfg.china_mirror ?? true);
    const newCfg: McpConfig = { ...cfg, china_mirror: next };
    try {
      await mcpSaveConfig(newCfg);
      setCfg(newCfg);
      setMsg(next ? "已开启国内镜像(npx→npmmirror,uvx/pip→清华 PyPI)" : "已关闭国内镜像");
    } catch (e: any) {
      setMsg(`失败: 保存镜像设置失败: ${e}`);
    }
  };

  /** 切换写入 / 执行工具与 MCP Server 开关(关闭后只下发只读内置工具)。 */
  const onToggleTools = async (checked: boolean) => {
    const newCfg: McpConfig = { ...cfg, tools_enabled: checked };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(
      checked
        ? "写入 / 执行工具与 MCP Server 已开启:模型可写文件、执行命令并调用 server 工具"
        : "写入 / 执行工具与 MCP Server 已关闭:仅保留只读内置工具",
    );
  };

  /** 切换内置工具总开关。 */
  const onToggleBuiltin = async (checked: boolean) => {
    const newCfg: McpConfig = { ...cfg, builtin_tools: checked };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(
      checked
        ? "内置工具已启用:模型可按需调用文件、系统、文档等工具"
        : "内置工具已关闭:不再下发任何内置工具",
    );
  };

  /** 切换审批档(全局永久设置,默认「逐次审批」)。 */
  const onPickMode = async (mode: ApprovalMode) => {
    const next = { ...policy, mode };
    setPolicy(next);
    try {
      await mcpSetApprovalPolicy(next);
      const title = APPROVAL_MODES.find((m) => m.id === mode)?.title ?? mode;
      setMsg(`工具权限已设为「${title}」,对所有会话永久生效`);
    } catch (e: any) {
      setMsg(`失败: 保存权限设置失败: ${e}`);
    }
  };

  const onSaveAllowList = async () => {
    const allow = allowText
      .split(/[\n;]/)
      .map((x) => x.trim())
      .filter(Boolean);
    setPolicy((p) => ({ ...p, allow }));
    try {
      await mcpSetApprovalPolicy({ ...policy, allow });
      setMsg(`附加放行白名单已保存(${allow.length} 项)`);
    } catch (e: any) {
      setMsg(`失败: 保存白名单失败: ${e}`);
    }
  };

  const refresh = async () => {
    const c = await mcpLoadConfig();
    setCfg(c);
    setMsg("");
  };

  /** C2:批量诊断所有 server,把结果 merge 到每张卡片的 status 里(精确保留已有 doctor)。 */
  const onDiagnose = async () => {
    setMsg("正在检查所有服务的运行状态…");
    try {
      const results = await mcpDoctor();
      setStatus((prev) => {
        const next = { ...prev };
        for (const dr of results) {
          const old = next[dr.name];
          next[dr.name] = {
            // 诊断结果是真实状态(is_running 来自 runtime handle),优先于前端旧值,
            // 否则进程已死时卡片仍显示「运行中」。
            running: dr.is_running,
            tools: old?.tools,
            doctor: dr,
          };
        }
        return next;
      });
      const bad = results.filter(
        (r) =>
          r.disabled_by_failure ||
          !r.command_resolved ||
          (r.is_running && !r.peek_ok && r.transport !== "http"),
      ).length;
      const ok = results.filter(
        (r) =>
          r.is_running &&
          r.command_resolved &&
          (r.peek_ok || r.transport === "http"),
      ).length;
      setMsg(
        bad > 0
          ? `检查完成:${results.length} 个服务中 ${ok} 正常,${bad} 个有问题(见卡片提示)`
          : `检查完成:全部 ${results.length} 个服务运行正常`,
      );
    } catch (e: any) {
      setMsg(`检查失败: ${e}`);
    }
  };

  const serverActions = useMcpServerActions({ cfg, setCfg, setStatus, setMsg, refresh });

  /** 添加 server:写入配置后重新加载列表并关闭对话框。 */
  const onAddServer = async (s: McpServerConfig) => {
    await mcpAddServer(s);
    await refresh();
    setShowAdd(false);
  };

  /** 更改默认工作区:写入任务根目录并刷新展示路径。 */
  const onPickWorkspace = async () => {
    const picked = await fsPickWorkspaceDir().catch(() => null);
    if (!picked || !settings) return;
    const next = { ...settings, workspace_root: picked };
    await ipc.saveSettings(next);
    setSettings(next);
    fsWorkspaceInfo()
      .then((i) => setWsRoot(i.root.replace(/^\\\\\?\\/, "")))
      .catch(() => {});
    setMsg("已更改默认工作区(下次工具调用生效)");
  };

  /** 恢复默认工作区(数据目录/workspace)。 */
  const onResetWorkspace = async () => {
    if (!settings) return;
    const next = { ...settings, workspace_root: null };
    await ipc.saveSettings(next);
    setSettings(next);
    fsWorkspaceInfo()
      .then((i) => setWsRoot(i.root.replace(/^\\\\\?\\/, "")))
      .catch(() => {});
    setMsg("已恢复默认工作区(数据目录/workspace)");
  };

  const onOpenWorkspace = () => fsOpenWorkspace().catch((e) => setMsg(`失败: 打开工作区失败: ${e}`));

  const onOpenBackups = () => fsOpenBackups().catch((e) => setMsg(`失败: 打开备份目录失败: ${e}`));

  return {
    cfg,
    status,
    loading,
    msg,
    showAdd,
    setShowAdd,
    policy,
    allowText,
    setAllowText,
    wsRoot,
    backupCount,
    settings,
    onToggleMirror,
    onToggleTools,
    onToggleBuiltin,
    onPickMode,
    onSaveAllowList,
    onAddServer,
    onPickWorkspace,
    onResetWorkspace,
    onOpenWorkspace,
    onOpenBackups,
    onDiagnose,
    ...serverActions,
  };
}
