import type { ApprovalMode, McpConfig } from '../../lib/ipc/mcp';
import type { AppSettings } from '../../lib/ipc/memory';

/** 工具能力卡片:下发开关 + 当前生效能力摘要(随审批模式实时变化)。 */
export function ToolCapabilityCard({
  cfg,
  settings,
  mode,
  onToggleTools,
  onToggleBuiltin,
  onToggleBareMode,
  onSetHumanVoice,
}: {
  cfg: McpConfig;
  settings: AppSettings | null;
  mode: ApprovalMode;
  onToggleTools: (enabled: boolean) => void;
  onToggleBuiltin: (enabled: boolean) => void;
  onToggleBareMode: (enabled: boolean) => void;
  onSetHumanVoice: (level: AppSettings['human_voice_level']) => void;
}) {
  const fullTrust = mode === 'full';
  const enabledServers = cfg.servers.filter((s) => s.enabled).length;
  return (
    <div className="mcp-time-card" style={{ marginBottom: 'var(--space-3)' }}>
      <div className="mcp-time-label">工具能力与下发</div>
      <label className="mcp-builtin-toggle">
        <input
          type="checkbox"
          checked={!!cfg.tools_enabled}
          onChange={(e) => onToggleTools(e.target.checked)}
        />
        <span>写入 / 执行工具 + MCP Server(关闭则只保留只读内置工具)</span>
      </label>
      <label className="mcp-builtin-toggle">
        <input
          type="checkbox"
          checked={!!cfg.builtin_tools}
          onChange={(e) => onToggleBuiltin(e.target.checked)}
        />
        <span>只读内置工具(取时间 / 列目录 / 读文件 / 检索 / 提取正文 / 列 zip)</span>
      </label>
      <label className="mcp-builtin-toggle">
        <input
          type="checkbox"
          checked={!!settings?.bare_mode}
          onChange={(e) => onToggleBareMode(e.target.checked)}
        />
        <span>自由人格模式:不追加「你是 myAgent 助手」身份锚(物理契约仍保留)</span>
      </label>
      <div className="mcp-builtin-toggle" style={{ flexWrap: 'wrap', cursor: 'default' }}>
        <span>文字质感(去 AI 味)</span>
        <select
          className="pg-field"
          style={{ width: 'auto', minWidth: 240 }}
          value={settings?.human_voice_level ?? 'standard'}
          disabled={!settings}
          onChange={(e) => onSetHumanVoice(e.target.value as AppSettings['human_voice_level'])}
        >
          <option value="standard">标准(推荐):去套话、拔高与客服腔</option>
          <option value="light">轻档:只去套话与拔高,保留原有结构</option>
          <option value="off">关闭:完全按智能体原提示输出</option>
        </select>
      </div>
      <div className="mcp-time-hint" style={{ marginTop: 'var(--space-1)' }}>
        仅对写作 / 问答 / 邮件 / 周报 / 报告等生成型智能体生效;
        代码、翻译、JSON、合规等场景自动跳过。事实、数据、引用与术语不受影响。
      </div>

      <div className="mcp-cap-list">
        <div className="mcp-cap-row">
          <span className="mcp-cap-name">文件</span>
          <b className="mcp-cap-val">全盘增删改查</b>
          <span className="mcp-cap-desc">
            无沙箱、无需授权目录:绝对路径直达本机任意位置,相对路径按文件工作区解析;
            覆盖 / 删除前自动备份,不设文件名黑名单
          </span>
        </div>
        <div className="mcp-cap-row">
          <span className="mcp-cap-name">系统命令</span>
          <b className="mcp-cap-val">{fullTrust ? '任意命令' : '只读白名单'}</b>
          <span className="mcp-cap-desc">
            {fullTrust
              ? '完全信任:shell 可执行任意命令,含环境变量 / 服务 / 注册表等系统环境操作'
              : '受限模式:仅只读命令白名单(路径不限目录),解释器与写操作被拒绝;需要系统级操作请切换到「完全信任」'}
          </span>
        </div>
        <div className="mcp-cap-row">
          <span className="mcp-cap-name">GUI 自动化</span>
          <b className="mcp-cap-val">{fullTrust ? '已解锁' : '关闭'}</b>
          <span className="mcp-cap-desc">窗口枚举 / 元素查找 / 点击输入,仅完全信任下可用</span>
        </div>
        <div className="mcp-cap-row">
          <span className="mcp-cap-name">MCP Server</span>
          <b className="mcp-cap-val">{enabledServers} 个已启用</b>
          <span className="mcp-cap-desc">
            {cfg.tools_enabled
              ? '启用的 server 工具随请求一起下发'
              : '总开关关闭,server 工具暂不下发'}
          </span>
        </div>
        {/* P14:外部 server 结果截断说明(本地 fs_* 不设上限) */}
        <div className="mcp-cap-row">
          <span className="mcp-cap-name">外部结果上限</span>
          <b className="mcp-cap-val">32k / 64k 字符</b>
          <span className="mcp-cap-desc">
            第三方 MCP Server 单条结果最多 3.2 万字符、单次调用合计 6.4 万字符,超出部分截断;
            本地 fs_* 文件工具不受此限(可分页读取 / 分批写入)
          </span>
        </div>
      </div>

      <div className="mcp-time-hint">
        当前底层提示词:{settings?.bare_mode ? '物理契约(身份锚已关闭)' : '身份锚 + 物理契约'}。
        正文由应用统一维护,此处不再重复展示。
      </div>
      <div className="mcp-time-hint">
        工具总开关只控制是否随请求下发工具;审批档位(逐次审批 / 工作区 / 完全信任)独立控制执行前是否询问,两者互不影响。
      </div>
    </div>
  );
}
