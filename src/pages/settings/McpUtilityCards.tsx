/** MCP 页的辅助卡片:长期记忆数据清理、国内镜像开关。 */

/** 长期记忆数据卡片:说明数据位置并提供清除入口。 */
export function MemoryDataCard({ onClear }: { onClear: () => void }) {
  return (
    <div className="mcp-time-card" style={{ marginBottom: 'var(--space-3)' }}>
      <div className="mcp-time-label">🧠 长期记忆数据</div>
      <div className="mcp-time-hint">
        「长期记忆」server 产生的全局记忆保存在 data\mcp-memory.jsonl,跨会话共享;
        仅禁用 server 不会删除历史数据。点此彻底清除(先停止相关进程再删除,不可恢复)。
      </div>
      <div className="mcp-server-actions">
        <button type="button" className="btn btn-xs" onClick={onClear}>
          清除长期记忆数据
        </button>
      </div>
    </div>
  );
}

/** 开箱即用卡片:国内镜像下载源开关。 */
export function MirrorCard({ chinaMirror, onToggle }: { chinaMirror: boolean | undefined; onToggle: () => void }) {
  return (
    <div className="mcp-policy-card">
      <div className="mcp-time-label">开箱即用</div>
      <label className="mcp-trust-row">
        <input
          type="checkbox"
          checked={chinaMirror ?? true}
          onChange={onToggle}
        />
        <span className="mcp-trust-body">
          <b>下载优先使用国内镜像</b>
          <span className="mcp-trust-desc">
            npx 首次启动 MCP server 走 npmmirror,uvx / pip 走清华 PyPI 镜像,避免官方源超时。
          </span>
        </span>
      </label>
    </div>
  );
}
