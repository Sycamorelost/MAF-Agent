import type { McpDoctorResult, McpServerConfig, McpToolInfo } from '../../lib/ipc/mcp';
import { serverDesc, toolDesc } from '../../lib/mcpCatalog';
import { IconX } from '../../components/icons';

/** 单个 server 的运行状态(running + 最近一次工具列表 + C2 诊断结果)。 */
export type McpServerStatus = {
  running: boolean;
  tools?: McpToolInfo[];
  /** C2:诊断结果,由 useMcpSettings.onDiagnose 注入 */
  doctor?: McpDoctorResult;
};

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
                <IconX size={14} />
              </button>
            </span>
          ))}
        </div>
      )}
      {/* C2 诊断状态 —— 仅在执行过诊断后显示 */}
      {st?.doctor && (
        <div style={{ marginTop: 'var(--space-1)', fontSize: 'var(--font-caption)', lineHeight: 1.6 }}>
          {st.doctor.disabled_by_failure && (
            <div className="mcp-warn" style={{ color: 'var(--err)' }}>
              该服务连续多次启动失败，已自动停用
              <button
                className="btn btn-xs btn-ghost"
                style={{ marginLeft: '6px', padding: '0 6px' }}
                onClick={() => onStart(s)}
              >
                手动重试
              </button>
            </div>
          )}
          {!st.doctor.disabled_by_failure && !st.doctor.command_resolved && (
            <div className="mcp-warn" style={{ color: 'var(--err)' }}>
              找不到启动命令 <code style={{ margin: '0 2px' }}>{s.command}</code>
              ，请确认已安装 Node.js 或 uvx，并加入系统 PATH
            </div>
          )}
          {st.doctor.is_running && !st.doctor.peek_ok && !st.doctor.disabled_by_failure && st.doctor.transport !== "http" && (
            <div className="mcp-warn" style={{ color: 'var(--warn)' }}>
              已启动但没响应，可能还在加载或已卡死
            </div>
          )}
          {!st.doctor.disabled_by_failure && st.doctor.is_running && (st.doctor.peek_ok || st.doctor.transport === "http") && (
            <div className="mcp-warn" style={{ color: 'var(--text-3)' }}>
              运行正常
              {st.doctor.is_preset && <span style={{ marginLeft: '4px', opacity: 0.7 }}>· 官方</span>}
              {st.doctor.backend !== "http" && (
                <span style={{ marginLeft: '4px', opacity: 0.7 }}>· {st.doctor.backend}</span>
              )}
            </div>
          )}
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
