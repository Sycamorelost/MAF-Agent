/** MCP 页的辅助卡片:国内镜像开关。 */

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
