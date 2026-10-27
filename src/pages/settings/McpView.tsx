import { useEffect, useState } from "react";
import {
  mcpLoadConfig,
  mcpSaveConfig,
  mcpAddServer,
  mcpRemoveServer,
  mcpStartServer,
  mcpListTools,
  mcpGetApprovalPolicy,
  mcpSetApprovalPolicy,
  getNow,
  type ApprovalMode,
  type ApprovalPolicy,
  type McpConfig,
  type McpServerConfig,
  type McpToolInfo,
  type NowResult,
} from "../../lib/ipc/mcp";
import { pickDataDir } from "../../lib/ipc";
import { serverDesc, toolDesc } from "../../lib/mcpCatalog";
import { FullTrustPanel } from "./FullTrustPanel";
import {
  fsBackupList,
  fsOpenBackups,
  fsOpenWorkspace,
  fsWorkspaceInfo,
} from "../../lib/ipc/fs";

/** 审批模式选项(对齐「如何批准 Agent 的操作」四档)。 */
const APPROVAL_MODES: { id: ApprovalMode; title: string; desc: string }[] = [
  { id: "manual", title: "手动审批", desc: "每次工具调用都由你确认" },
  { id: "auto", title: "自动审批", desc: "只读查询自动放行，写入 / 执行仍询问" },
  { id: "full", title: "完全访问", desc: "不再弹窗，任意命令直接运行（敏感黑名单与审计仍生效）" },
  { id: "custom", title: "自定义", desc: "命中下方白名单的自动放行，其余询问" },
];

/** MCP 设置页:server 列表 + 启动/测试 + 添加对话框。 */
export function McpView() {
  const [cfg, setCfg] = useState<McpConfig>({ servers: [] });
  const [status, setStatus] = useState<Record<string, { running: boolean; tools?: McpToolInfo[] }>>({});
  const [loading, setLoading] = useState(true);
  const [msg, setMsg] = useState<string>("");
  const [now, setNow] = useState<NowResult | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [policy, setPolicy] = useState<ApprovalPolicy>({ mode: "manual", allow: [] });
  const [allowText, setAllowText] = useState("");
  /** 文件工作区根目录与备份数量(内置 fs 工具的输出位置) */
  const [wsRoot, setWsRoot] = useState("");
  const [backupCount, setBackupCount] = useState(0);

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
      setMsg(`❌ 保存镜像设置失败: ${e}`);
    }
  };

  /** 切换审批模式(「解放双手」:自动 / 完全访问可让 Agent 连续执行)。 */
  const onPickMode = async (mode: ApprovalMode) => {
    setPolicy((p) => ({ ...p, mode }));
    try {
      await mcpSetApprovalPolicy({ ...policy, mode });
      const title = APPROVAL_MODES.find((m) => m.id === mode)?.title ?? mode;
      setMsg(`审批模式已切换为「${title}」`);
    } catch (e: any) {
      setMsg(`❌ 保存审批模式失败: ${e}`);
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
      setMsg(`自定义白名单已保存(${allow.length} 项)`);
    } catch (e: any) {
      setMsg(`❌ 保存白名单失败: ${e}`);
    }
  };

  // 内置时间工具测试 —— Phase 6
  useEffect(() => {
    getNow().then(setNow).catch(() => {});
  }, []);

  const refresh = async () => {
    const c = await mcpLoadConfig();
    setCfg(c);
    setMsg("");
  };

  const onToggleEnabled = async (s: McpServerConfig) => {
    const next = { ...s, enabled: !s.enabled };
    const newCfg: McpConfig = {
      ...cfg,
      servers: cfg.servers.map((x) => (x.name === s.name ? next : x)),
    };
    await mcpSaveConfig(newCfg);
    setCfg(newCfg);
    setMsg(`${next.name} ${next.enabled ? "已启用" : "已禁用"}`);
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
      "允许目录(多个用分号 ; 分隔;留空则回落到沙箱目录 data\\mcp-sandbox):",
      s.allowed_dirs.join("; "),
    );
    if (input === null) return;
    const dirs = input.split(/[;\n]/).map((x) => x.trim()).filter(Boolean);
    await saveDirs(s, dirs);
    setMsg(dirs.length > 0 ? `${s.name}: 已设置 ${dirs.length} 个允许目录,请重新启动` : `${s.name}: 已清空允许目录,将使用沙箱目录`);
  };

  /** 目录选择器添加允许目录(可授权任意文件夹,全盘读写由审批与黑名单兜底)。 */
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

  if (loading) return <div className="set-hint">加载中…</div>;

  return (
    <div className="mcp-view">
      <h2>MCP 工具</h2>
      <p className="set-desc">
        通过 MCP(Model Context Protocol)安全接入本地工具,实现文件读写、命令执行等能力。
        所有 server 需用户主动启用,默认关闭。
      </p>
      {msg && (
        <div className={`mcp-msg${msg.startsWith("❌") || msg.startsWith("删除失败") ? " err" : ""}`}>
          {msg}
        </div>
      )}

      <div className="mcp-time-card" style={{ marginBottom: 'var(--space-3)' }}>
        <div className="mcp-time-label">🔌 对话工具调用(总开关)</div>
        <label className="mcp-builtin-toggle">
          <input
            type="checkbox"
            checked={!!cfg.tools_enabled}
            onChange={async (e) => {
              const newCfg: McpConfig = { ...cfg, tools_enabled: e.target.checked };
              await mcpSaveConfig(newCfg);
              setCfg(newCfg);
              setMsg(
                e.target.checked
                  ? "工具调用已开启:模型可调用下方 server 与内置工具"
                  : "工具调用已关闭:每次请求不再下发工具定义,可显著降低 token 与首字延迟",
              );
            }}
          />
          <span>向模型下发工具定义(关闭可省数千 token/次,纯对话更快)</span>
        </label>
        <div className="mcp-time-hint">
          关闭后模型将看不到任何工具(内置工具与下方所有 MCP server),
          只做纯文本对话;需要文件、命令或联网能力时再打开。
        </div>
      </div>

      {now && (
        <div className="mcp-time-card">
          <div className="mcp-time-label">⏰ 内置时间工具(不走 MCP)</div>
          <div className="mcp-time-val">
            {now.date} {now.time} · {now.weekday} ({now.tz})
          </div>
          <label className="mcp-builtin-toggle">
            <input
              type="checkbox"
              checked={!!cfg.builtin_tools}
              onChange={async (e) => {
                const newCfg: McpConfig = { ...cfg, builtin_tools: e.target.checked };
                await mcpSaveConfig(newCfg);
                setCfg(newCfg);
                setMsg(
                  e.target.checked
                    ? "内置工具已启用:模型可调用 取当前时间 / 白名单 shell(执行前需你确认)"
                    : "内置工具已关闭",
                );
              }}
            />
            <span>允许模型调用内置工具(取时间 / 白名单 shell 只读命令)</span>
          </label>
          <div className="mcp-time-hint">
            默认仅支持工作区内只读命令;开启上方「完全信任 MCP」后 shell 可执行任意命令;文件工具在
            <strong> 文件工作区 </strong>
            内读写,覆盖/删除前自动备份。
          </div>
        </div>
      )}

      <div className="mcp-policy-card">
        <div className="mcp-time-label">文件工作区</div>
        <div className="mcp-policy-desc">
          Agent 读写的文件统一位于此目录(可用环境变量 MAF_WORKSPACE 覆盖);覆盖或删除前会自动备份,
          可在备份目录中找回。
        </div>
        <code className="mcp-ws-path">{wsRoot || "…"}</code>
        <div className="mcp-server-actions">
          <button
            type="button"
            className="btn btn-xs"
            onClick={() => fsOpenWorkspace().catch((e) => setMsg(`❌ 打开工作区失败: ${e}`))}
          >
            打开工作区
          </button>
          <button
            type="button"
            className="btn btn-xs"
            onClick={() => fsOpenBackups().catch((e) => setMsg(`❌ 打开备份目录失败: ${e}`))}
          >
            备份目录({backupCount})
          </button>
        </div>
      </div>

      <div className="mcp-policy-card">
        <div className="mcp-time-label">开箱即用</div>
        <label className="mcp-trust-row">
          <input
            type="checkbox"
            checked={policy.mode === "full"}
            onChange={() => onPickMode(policy.mode === "full" ? "manual" : "full")}
          />
          <span className="mcp-trust-body">
            <b>完全信任 MCP</b>
            <span className="mcp-trust-desc">
              开启后 Agent 调用任何工具不再弹窗,shell 可执行任意命令(装 JDK、跑脚本等,
              <strong>不再受白名单与工作区路径限制</strong>),超时 5 分钟;同时解锁 GUI 自动化与临时授权。
              文件工作区沙箱(路径穿越/敏感文件黑名单)与审计日志始终生效。关闭即回到「手动审批」。
            </span>
          </span>
        </label>
        <label className="mcp-trust-row">
          <input
            type="checkbox"
            checked={cfg?.china_mirror ?? true}
            onChange={onToggleMirror}
          />
          <span className="mcp-trust-body">
            <b>下载优先使用国内镜像</b>
            <span className="mcp-trust-desc">
              npx 首次启动 MCP server 走 npmmirror,uvx / pip 走清华 PyPI 镜像,避免官方源超时。
            </span>
          </span>
        </label>
      </div>

      <div className="mcp-policy-card">
        <div className="mcp-time-label">如何批准 Agent 的操作?</div>
        <div className="mcp-policy-desc">
          「自动审批」让只读工具免弹窗;「完全访问」等同上方「完全信任 MCP」——不再弹窗,
          shell 放开白名单与路径限制可执行任意命令(文件沙箱与审计仍生效),并解锁 GUI 自动化与临时授权。
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
            已开启完全访问:工具调用不再询问,shell 可执行任意命令(含工作区外路径),GUI 自动化已解锁。
            请仅在同时信任模型与全部工具输出(网页 / 文件内容)时使用。
          </div>
        )}
        {policy.mode === "custom" && (
          <label className="mcp-field mcp-allow-field">
            <span>自动放行的工具(每行一条,如 项目文件::read_text_file 或 read_text_file)</span>
            <textarea
              className="mcp-allow-input"
              rows={4}
              value={allowText}
              onChange={(e) => setAllowText(e.target.value)}
              placeholder={"项目文件::read_text_file\n长期记忆::search_nodes"}
            />
            <button className="btn btn-xs" onClick={onSaveAllowList}>
              保存白名单
            </button>
          </label>
        )}
      </div>

      <FullTrustPanel fullTrust={policy.mode === "full"} />

      <div className="mcp-servers">
        {cfg.servers.length === 0 && (
          <div className="set-hint">暂无 MCP server,点击下方按钮添加</div>
        )}
        {cfg.servers.map((s) => {
          const st = status[s.name];
          return (
            <div key={s.name} className="mcp-server-card">
              <div className="mcp-server-head">
                <div className="mcp-server-title">
                  <span className={`status-dot ${s.enabled ? "on" : "off"}`} />
                  <strong>{s.name}</strong>
                  <span className="mcp-transport-tag">{s.transport === "http" ? "HTTP" : "stdio"}</span>
                  {s.transport === "http" ? (
                    <code className="mcp-cmd" title={s.url}>{s.url || "(未配置 URL)"}</code>
                  ) : (
                    <code className="mcp-cmd">{s.command} {s.args.slice(0, 2).join(" ")}{s.args.length > 2 ? " …" : ""}</code>
                  )}
                </div>
                <div className="mcp-server-actions">
                  {s.transport !== "http" && s.args.some((a) => a.includes("server-filesystem")) && (
                    <>
                      <button className="btn btn-xs" onClick={() => onAddDir(s)}>
                        选择目录…
                      </button>
                      <button className="btn btn-xs btn-ghost" onClick={() => onEditDirs(s)}>
                        手填…
                      </button>
                    </>
                  )}
                  <button
                    className="btn btn-xs"
                    onClick={() => onToggleEnabled(s)}
                  >
                    {s.enabled ? "禁用" : "启用"}
                  </button>
                  <button
                    className="btn btn-xs btn-primary"
                    disabled={st?.running}
                    onClick={() => onStart(s)}
                  >
                    {st?.running ? "运行中" : "启动 + 测试"}
                  </button>
                  <button
                    className="btn btn-xs btn-ghost"
                    onClick={() => onRemove(s.name)}
                  >
                    删除
                  </button>
                </div>
              </div>
              {serverDesc(s.name) && <div className="mcp-server-desc">{serverDesc(s.name)}</div>}
              {s.transport !== "http" && s.args.some((a) => a.includes("server-filesystem")) && s.allowed_dirs.length === 0 && (
                <div className="mcp-warn">
                  未指定允许目录,默认仅可读写沙箱目录 data\mcp-sandbox;点「选择目录…」可授权任意文件夹
                </div>
              )}
              {s.transport === "http" && (s.headers && Object.keys(s.headers).length > 0) && (
                <div className="mcp-allowed">
                  🔑 请求头:
                  {Object.keys(s.headers).map((k) => (
                    <span key={k} className="mcp-chip">{k}</span>
                  ))}
                </div>
              )}
              {s.allowed_dirs.length > 0 && (
                <div className="mcp-allowed">
                  📁 允许目录:
                  {s.allowed_dirs.map((d) => (
                    <span key={d} className="mcp-chip">
                      {d}
                      <button
                        className="mcp-chip-x"
                        title="移除该目录"
                        onClick={() => onRemoveDir(s, d)}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                </div>
              )}
              {st?.tools && (
                <details className="mcp-tools">
                  <summary>工具列表({st.tools.length})</summary>
                  <ul>
                    {st.tools.map((t) => (
                      <li key={t.name} className="mcp-tool-item" title={t.description}>
                        <code>{t.name}</code>
                        <span className="mcp-tool-desc">{toolDesc(t.name, t.description)}</span>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
          );
        })}
      </div>

      <div className="mcp-foot">
        <button className="btn btn-primary" onClick={() => setShowAdd(true)}>
          + 添加 MCP Server
        </button>
      </div>

      {showAdd && (
        <AddServerDialog
          onClose={() => setShowAdd(false)}
          onAdd={async (s) => {
            await mcpAddServer(s);
            await refresh();
            setShowAdd(false);
          }}
        />
      )}
    </div>
  );
}

/** 添加 server 对话框 —— stdio(本地子进程)/ http(远端 Streamable HTTP)。 */
function AddServerDialog({ onClose, onAdd }: { onClose: () => void; onAdd: (s: McpServerConfig) => Promise<void> }) {
  const [name, setName] = useState("");
  const [transport, setTransport] = useState<"stdio" | "http">("stdio");
  const [command, setCommand] = useState("npx");
  const [args, setArgs] = useState("-y @modelcontextprotocol/server-filesystem");
  const [dir, setDir] = useState("");
  const [url, setUrl] = useState("");
  const [headersText, setHeadersText] = useState("");

  const onPickDir = async () => {
    // plugin-dialog 未集成;手动在输入框里填路径即可
    setDir((prompt("请输入允许的目录绝对路径:") || "").trim());
  };

  /** 每行 `Key: Value`,空行忽略。 */
  const parseHeaders = (): Record<string, string> => {
    const out: Record<string, string> = {};
    for (const line of headersText.split(/\r?\n/)) {
      const i = line.indexOf(":");
      if (i <= 0) continue;
      const k = line.slice(0, i).trim();
      const v = line.slice(i + 1).trim();
      if (k && v) out[k] = v;
    }
    return out;
  };

  const submit = async () => {
    if (!name) return;
    if (transport === "stdio" && !command) return;
    if (transport === "http" && !url.trim()) return;
    await onAdd({
      name: name.trim(),
      enabled: false,
      transport,
      command: transport === "stdio" ? command.trim() : "",
      args: transport === "stdio" ? args.split(/\s+/).filter(Boolean) : [],
      url: transport === "http" ? url.trim() : "",
      headers: transport === "http" ? parseHeaders() : {},
      allowed_dirs: transport === "stdio" && dir ? [dir] : [],
      deny_patterns: [".env", "*.pem", "*credentials*.json"],
    });
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog mcp-add-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">添加 MCP Server</div>
        <div className="dialog-body mcp-add-body">
          <label className="mcp-field">
            <span>名称</span>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="项目文件" />
          </label>
          <label className="mcp-field">
            <span>传输方式</span>
            <select
              value={transport}
              onChange={(e) => setTransport(e.target.value as "stdio" | "http")}
            >
              <option value="stdio">stdio —— 本地子进程(npx / node)</option>
              <option value="http">HTTP —— 远端 Streamable HTTP 端点</option>
            </select>
          </label>
          {transport === "stdio" ? (
            <>
              <label className="mcp-field">
                <span>命令</span>
                <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="npx" />
              </label>
              <label className="mcp-field">
                <span>参数</span>
                <input value={args} onChange={(e) => setArgs(e.target.value)} placeholder="-y @modelcontextprotocol/server-filesystem" />
              </label>
              <label className="mcp-field">
                <span>允许目录(文件系统工具必须配置)</span>
                <div className="mcp-field-row">
                  <input value={dir} onChange={(e) => setDir(e.target.value)} placeholder="C:\\path\\to\\project" />
                  <button className="btn btn-xs" onClick={onPickDir}>选择…</button>
                </div>
              </label>
            </>
          ) : (
            <>
              <label className="mcp-field">
                <span>端点 URL(https;本机可用 http://127.0.0.1:端口)</span>
                <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://mcp.example.com/mcp" />
              </label>
              <label className="mcp-field">
                <span>请求头(每行 Key: Value,如 Authorization: Bearer xxx;明文存储)</span>
                <textarea
                  className="mcp-allow-input"
                  rows={3}
                  value={headersText}
                  onChange={(e) => setHeadersText(e.target.value)}
                  placeholder={"Authorization: Bearer <token>\nX-API-Key: <key>"}
                />
              </label>
            </>
          )}
        </div>
        <div className="mcp-add-actions">
          <button className="btn btn-ghost" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={submit}>添加</button>
        </div>
      </div>
    </div>
  );
}
