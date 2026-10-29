import type { McpServerConfig, McpToolInfo } from '../../lib/ipc/mcp';
import { serverDesc, toolDesc } from '../../lib/mcpCatalog';

/** 单个 server 的运行状态(running + 最近一次工具列表)。 */
export type McpServerStatus = { running: boolean; tools?: McpToolInfo[] };

/** MCP server 卡片:状态、启停、允许目录与工具列表。 */
export function McpServerCard({
  server: s,
  st,
  onToggleEnabled,
  onStart,
  onRemove,
  onAddDir,
  onEditDirs,
  onRemoveDir,
}: {
  server: McpServerConfig;
  st?: McpServerStatus;
  onToggleEnabled: (s: McpServerConfig) => void;
  onStart: (s: McpServerConfig) => void;
  onRemove: (name: string) => void;
  onAddDir: (s: McpServerConfig) => void;
  onEditDirs: (s: McpServerConfig) => void;
  onRemoveDir: (s: McpServerConfig, dir: string) => void;
}) {
  return (
    <div className="mcp-server-card">
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
          未指定允许目录,默认不设沙箱(可读写整机所有磁盘);点「选择目录…」可主动收窄范围
        </div>
      )}
      {s.transport === "http" && (s.headers && Object.keys(s.headers).length > 0) && (
        <div className="mcp-allowed">
          请求头:
          {Object.keys(s.headers).map((k) => (
            <span key={k} className="mcp-chip">{k}</span>
          ))}
        </div>
      )}
      {s.allowed_dirs.length > 0 && (
        <div className="mcp-allowed">
          允许目录:
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
}
