import { useState } from 'react';
import type { McpServerConfig } from '../../lib/ipc/mcp';

/** 添加 server 对话框 —— stdio(本地子进程)/ http(远端 Streamable HTTP)。 */
export function AddServerDialog({ onClose, onAdd }: { onClose: () => void; onAdd: (s: McpServerConfig) => Promise<void> }) {
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
                <span>允许目录(可选;留空 = 不设沙箱,整机可读写)</span>
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
