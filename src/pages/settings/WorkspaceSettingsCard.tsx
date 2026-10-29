/** 文件工作区卡片:相对路径的基准目录、备份数量与更改 / 恢复 / 打开操作。 */
export function WorkspaceSettingsCard({
  wsRoot,
  backupCount,
  onChange,
  onReset,
  onOpenWorkspace,
  onOpenBackups,
}: {
  wsRoot: string;
  backupCount: number;
  onChange: () => void;
  onReset: () => void;
  onOpenWorkspace: () => void;
  onOpenBackups: () => void;
}) {
  return (
    <div className="mcp-policy-card">
      <div className="mcp-time-label">文件工作区</div>
      <div className="mcp-policy-desc">
        相对路径的基准目录。Agent 也可用绝对路径直接操作本机任意文件;覆盖或删除前会自动备份,
        可在备份目录中找回。留空时默认用「数据目录/workspace」;环境变量
        <code> MAF_WORKSPACE </code>仍优先。
      </div>
      <code className="mcp-ws-path">{wsRoot || "…"}</code>
      <div className="mcp-server-actions">
        <button
          type="button"
          className="btn btn-xs"
          onClick={onChange}
        >
          更改
        </button>
        <button
          type="button"
          className="btn btn-xs"
          onClick={onReset}
        >
          恢复默认
        </button>
        <button
          type="button"
          className="btn btn-xs"
          onClick={onOpenWorkspace}
        >
          打开工作区
        </button>
        <button
          type="button"
          className="btn btn-xs"
          onClick={onOpenBackups}
        >
          备份目录({backupCount})
        </button>
      </div>
    </div>
  );
}
