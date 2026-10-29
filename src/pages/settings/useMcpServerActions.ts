import { useState, type Dispatch, type SetStateAction } from 'react';
import {
  mcpSaveConfig,
  mcpRemoveServer,
  mcpStartServer,
  mcpStartAllEnabled,
  mcpListTools,
  type McpConfig,
  type McpServerConfig,
} from '../../lib/ipc/mcp';
import { pickDataDir } from '../../lib/ipc';
import type { McpServerStatus } from './McpServerCard';

/** MCP server 级动作:启停、启动测试、一键启动、允许目录与删除。 */
export function useMcpServerActions({
  cfg,
  setCfg,
  setStatus,
  setMsg,
  refresh,
}: {
  cfg: McpConfig;
  setCfg: Dispatch<SetStateAction<McpConfig>>;
  setStatus: Dispatch<SetStateAction<Record<string, McpServerStatus>>>;
  setMsg: Dispatch<SetStateAction<string>>;
  refresh: () => Promise<void>;
}) {
  /** 一键启动进行中 */
  const [startingAll, setStartingAll] = useState(false);

  const onToggleEnabled = async (s: McpServerConfig) => {
    const next = { ...s, enabled: !s.enabled };
    const newCfg: McpConfig = {
      ...cfg,
      servers: cfg.servers.map((x) => (x.name === s.name ? next : x)),
    };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(
      next.enabled
        ? `${next.name} 已启用`
        : `${next.name} 已禁用,运行中的进程已停止`,
    );
  };

  const onStart = async (s: McpServerConfig) => {
    setMsg(`正在启动 ${s.name}…`);
    try {
      // stdio 会 spawn 子进程,http 只做远端握手;参数统一取自已保存配置
      await mcpStartServer(s.name);
      // 启动后立即 list_tools 测试
      const tools = await mcpListTools(s.name);
      setStatus((prev) => ({ ...prev, [s.name]: { running: true, tools } }));
      setMsg(`✅ ${s.name} 启动成功,共 ${tools.length} 个工具`);
    } catch (e: any) {
      setMsg(`❌ 启动失败: ${e}`);
      setStatus((prev) => ({ ...prev, [s.name]: { running: false } }));
    }
  };

  /** 一键启动所有 enabled 的 MCP server。 */
  const onStartAll = async () => {
    const enabled = cfg.servers.filter((s) => s.enabled);
    if (enabled.length === 0) {
      setMsg("没有已启用的 server,请先勾选启用");
      return;
    }
    setStartingAll(true);
    setMsg(`正在一键启动 ${enabled.length} 个 server…`);
    try {
      const [ok, fail, errors] = await mcpStartAllEnabled();
      if (fail === 0) {
        setMsg(`✅ 一键启动完成:${ok} 个 server 全部启动成功`);
      } else {
        setMsg(`⚠️ 一键启动完成:${ok} 成功,${fail} 失败${errors.length ? `(${errors.join("; ")})` : ""}`);
      }
      // 刷新状态
      const newStatus: Record<string, McpServerStatus> = {};
      for (const s of enabled) {
        try {
          const tools = await mcpListTools(s.name);
          newStatus[s.name] = { running: true, tools };
        } catch {
          newStatus[s.name] = { running: false };
        }
      }
      setStatus((prev) => ({ ...prev, ...newStatus }));
    } catch (e: any) {
      setMsg(`❌ 一键启动失败: ${e}`);
    } finally {
      setStartingAll(false);
    }
  };

  /** 保存允许目录并标记需重启 —— 启动参数与安全预检共用同一份目录列表。 */
  const saveDirs = async (s: McpServerConfig, dirs: string[]) => {
    const newCfg: McpConfig = {
      ...cfg,
      servers: cfg.servers.map((x) => (x.name === s.name ? { ...x, allowed_dirs: dirs } : x)),
    };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setStatus((prev) => ({ ...prev, [s.name]: { running: false } }));
  };

  /** 文件系统 server 的允许目录只能在这里改 —— 改完需重新启动才生效。 */
  const onEditDirs = async (s: McpServerConfig) => {
    const input = prompt(
      "允许目录(多个用分号 ; 分隔;留空则不设沙箱,整机所有磁盘可读写):",
      s.allowed_dirs.join("; "),
    );
    if (input === null) return;
    const dirs = input.split(/[;\n]/).map((x) => x.trim()).filter(Boolean);
    await saveDirs(s, dirs);
    setMsg(dirs.length > 0 ? `${s.name}: 已设置 ${dirs.length} 个允许目录,请重新启动` : `${s.name}: 已清空允许目录,将不设沙箱(整机可读写)`);
  };

  /** 目录选择器添加允许目录(可授权任意文件夹,全盘读写由审批与审计兜底)。 */
  const onAddDir = async (s: McpServerConfig) => {
    const p = await pickDataDir();
    if (!p) return;
    if (s.allowed_dirs.some((d) => d.toLowerCase() === p.toLowerCase())) {
      setMsg(`${s.name}: 目录已在允许列表中`);
      return;
    }
    await saveDirs(s, [...s.allowed_dirs, p]);
    setMsg(`${s.name}: 已添加允许目录,请重新启动`);
  };

  const onRemoveDir = async (s: McpServerConfig, dir: string) => {
    await saveDirs(s, s.allowed_dirs.filter((x) => x !== dir));
    setMsg(`${s.name}: 已移除目录,请重新启动`);
  };

  const onRemove = async (name: string) => {
    if (!confirm(`删除 server \`${name}\` ?`)) return;
    try {
      await mcpRemoveServer(name);
      await refresh();
    } catch (e: any) {
      setMsg(`删除失败: ${e}`);
    }
  };

  return {
    startingAll,
    onToggleEnabled,
    onStart,
    onStartAll,
    onEditDirs,
    onAddDir,
    onRemoveDir,
    onRemove,
  };
}
