import { useEffect, useState } from 'react';
import {
  mcpLoadConfig,
  mcpSaveConfig,
  mcpClearMemory,
  mcpAddServer,
  mcpGetApprovalPolicy,
  mcpSetApprovalPolicy,
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
  const [policy, setPolicy] = useState<ApprovalPolicy>({ mode: "workspace", allow: [] });
  const [allowText, setAllowText] = useState("");
  /** 文件工作区根目录与备份数量(内置 fs 工具的输出位置) */
  const [wsRoot, setWsRoot] = useState("");
  const [backupCount, setBackupCount] = useState(0);
  /** 应用设置(自由人格模式开关,读取+单字段保存) */
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

  /** 切换工具下发总开关(关闭后不再下发任何工具定义)。 */
  const onToggleTools = async (checked: boolean) => {
    const newCfg: McpConfig = { ...cfg, tools_enabled: checked };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(
      checked
        ? "工具调用已开启:模型可调用下方 server 与内置工具"
        : "工具调用已关闭:每次请求不再下发工具定义,可显著降低 token 与首字延迟",
    );
  };

  /** 切换自由人格模式(单字段保存应用设置)。 */
  const onToggleBareMode = async (checked: boolean) => {
    if (!settings) return;
    const next = { ...settings, bare_mode: checked };
    await ipc.saveSettings(next);
    setSettings(next);
    setMsg(
      checked
        ? "自由人格模式已开启:system 仅保留工具物理契约,身份/人格完全交给智能体或中转模型"
        : "身份锚已恢复:system 追加「你是 myAgent 助手」",
    );
  };

  /** 切换内置工具开关(取时间 / 白名单 shell)。 */
  const onToggleBuiltin = async (checked: boolean) => {
    const newCfg: McpConfig = { ...cfg, builtin_tools: checked };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(
      checked
        ? "内置工具已启用:模型可调用 取当前时间 / 白名单 shell(执行前需你确认)"
        : "内置工具已关闭",
    );
  };

  /** 切换审批档(D11 三档:逐次 / 工作区 / 完全信任)。 */
  const onPickMode = async (mode: ApprovalMode) => {
    setPolicy((p) => ({ ...p, mode }));
    try {
      await mcpSetApprovalPolicy({ ...policy, mode });
      const title = APPROVAL_MODES.find((m) => m.id === mode)?.title ?? mode;
      setMsg(`审批档已切换为「${title}」`);
    } catch (e: any) {
      setMsg(`失败: 保存审批档失败: ${e}`);
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

  /** 清除「长期记忆」全局数据:先停 server 进程,再删 mcp-memory.jsonl。 */
  const onClearMemory = async () => {
    if (!confirm("确定清除全部长期记忆数据?\n该数据由「长期记忆」MCP 工具跨会话共享,删除后不可恢复。")) return;
    try {
      const size = await mcpClearMemory();
      setMsg(
        size > 0
          ? `成功: 已清除长期记忆数据(${(size / 1024).toFixed(1)} KB),相关 server 已停止`
          : "长期记忆数据为空,无需清除",
      );
    } catch (e: any) {
      setMsg(`失败: 清除失败: ${e}`);
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
    onToggleBareMode,
    onToggleBuiltin,
    onPickMode,
    onSaveAllowList,
    onClearMemory,
    onAddServer,
    onPickWorkspace,
    onResetWorkspace,
    onOpenWorkspace,
    onOpenBackups,
    ...serverActions,
  };
}
