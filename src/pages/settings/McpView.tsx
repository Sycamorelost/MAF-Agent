import { FullTrustPanel } from './FullTrustPanel';
import { AddServerDialog } from './AddServerDialog';
import { ApprovalPolicyCard } from './ApprovalPolicyCard';
import { McpServerCard } from './McpServerCard';
import { ToolCapabilityCard } from './ToolCapabilityCard';
import { MemoryDataCard, MirrorCard } from './McpUtilityCards';
import { WorkspaceSettingsCard } from './WorkspaceSettingsCard';
import { useMcpSettings } from './useMcpSettings';

/** MCP 设置页:能力总览 + 文件工作区 + 审批模式 + server 列表。 */
export function McpView() {
  const {
    cfg,
    status,
    loading,
    msg,
    showAdd,
    setShowAdd,
    policy,
    allowText,
    setAllowText,
    startingAll,
    wsRoot,
    backupCount,
    settings,
    onToggleMirror,
    onToggleTools,
    onToggleBareMode,
    onToggleBuiltin,
    onPickMode,
    onSaveAllowList,
    onStartAll,
    onEditDirs,
    onAddDir,
    onRemoveDir,
    onRemove,
    onStart,
    onToggleEnabled,
    onClearMemory,
    onAddServer,
    onPickWorkspace,
    onResetWorkspace,
    onOpenWorkspace,
    onOpenBackups,
  } = useMcpSettings();

  if (loading) return <div className="set-hint">加载中…</div>;

  return (
    <div className="mcp-view">
      <h2>MCP 工具</h2>
      <p className="set-desc">
        内置文件工具默认具备本机全盘增删改查(无沙箱、无文件名黑名单,覆盖 / 删除前自动备份);
        系统命令按下方审批模式放行。通过 MCP 接入的外部 server 需手动启用,默认关闭。
      </p>
      {msg && (
        <div className={`mcp-msg${msg.startsWith("错误") || msg.includes("失败") ? " err" : ""}`}>
          {msg}
        </div>
      )}

      <ToolCapabilityCard
        cfg={cfg}
        settings={settings}
        mode={policy.mode}
        onToggleTools={onToggleTools}
        onToggleBuiltin={onToggleBuiltin}
        onToggleBareMode={onToggleBareMode}
      />

      <WorkspaceSettingsCard
        wsRoot={wsRoot}
        backupCount={backupCount}
        onChange={onPickWorkspace}
        onReset={onResetWorkspace}
        onOpenWorkspace={onOpenWorkspace}
        onOpenBackups={onOpenBackups}
      />

      <ApprovalPolicyCard
        policy={policy}
        allowText={allowText}
        onAllowTextChange={setAllowText}
        onPickMode={onPickMode}
        onSaveAllowList={onSaveAllowList}
      />

      <FullTrustPanel fullTrust={policy.mode === "full"} />

      <MemoryDataCard onClear={onClearMemory} />

      <MirrorCard chinaMirror={cfg?.china_mirror} onToggle={onToggleMirror} />

      <div className="mcp-servers">
        {cfg.servers.length === 0 && (
          <div className="set-hint">暂无 MCP server,点击下方按钮添加</div>
        )}
        {cfg.servers.map((s) => (
          <McpServerCard
            key={s.name}
            server={s}
            st={status[s.name]}
            onAddDir={onAddDir}
            onEditDirs={onEditDirs}
            onToggleEnabled={onToggleEnabled}
            onStart={onStart}
            onRemove={onRemove}
            onRemoveDir={onRemoveDir}
          />
        ))}
      </div>

      <div className="mcp-foot" style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <button className="btn btn-primary" onClick={() => setShowAdd(true)}>
          + 添加 MCP Server
        </button>
        <button
          className="btn btn-primary"
          disabled={startingAll || cfg.servers.filter((s) => s.enabled).length === 0}
          onClick={() => void onStartAll()}
        >
          {startingAll ? '正在一键启动…' : `一键启动全部 (${cfg.servers.filter((s) => s.enabled).length})`}
        </button>
      </div>

      {showAdd && (
        <AddServerDialog
          onClose={() => setShowAdd(false)}
          onAdd={onAddServer}
        />
      )}
    </div>
  );
}
