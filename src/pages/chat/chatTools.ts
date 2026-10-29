import type { AgentTemplate, ReasoningCapability, ToolDef } from '../../lib/ipc';
import { mcpCollectTools } from '../../lib/ipc/mcp';
import type { ThinkingSelection } from '../../stores/thinking';
import { DEFAULT_THINKING_LEVEL } from '../../stores/thinking';

/** 计算本次流式请求的思考档位(模型不支持 → null;用户关闭 → 'off';否则档位)。 */
export function resolveReasoningEffort(
  thinkCap: ReasoningCapability | null,
  thinkingSel: ThinkingSelection | undefined,
): string | null {
  // 思考参数:模型不支持 → null(后端能力门控二次兜底);用户关闭 → 'off';否则档位
  return !thinkCap?.enabled
    ? null
    : thinkingSel && !thinkingSel.on
      ? 'off'
      : (thinkingSel?.level ?? DEFAULT_THINKING_LEVEL);
}

/** 收集本次可用的 MCP 工具定义(按智能体工具策略过滤;空则返回 undefined 表示不启用工具调用)。 */
export async function collectSessionTools(agent: AgentTemplate | null): Promise<ToolDef[] | undefined> {
  // Phase 5e:收集已启用 MCP server 的工具定义;空则不启用工具调用(行为不变)
  let tools: ToolDef[] | undefined;
  try {
    let collected = await mcpCollectTools();
    // 智能体工具策略(R3 A6):null=继承全局;空数组=禁用工具;列表=仅允许名单内工具
    const allow = agent?.tool_policy ?? null;
    if (allow) {
      const names = new Set(allow);
      collected = collected.filter((t) => names.has(t.function.name));
    }
    if (collected.length > 0) tools = collected;
  } catch {
    /* MCP 未配置/未启动 —— 静默降级 */
  }
  return tools;
}
