import * as ipc from '../../lib/ipc';
import type { MemoryStrategy } from '../../lib/ipc';
import { injectByStrategy, enforceBudget, MEMORY_BUDGET_TOKENS } from '../../lib/memoryInject';
import { useMemoryStore } from '../../stores/memory';

/** 正在提炼的会话,避免并发重复提炼。 */
const summarizing = new Set<string>();

/** 需要时后台自动提炼记忆:needs_summarize → summarize_memory → write_memory → 刷新面板。
 *  D3.7:优先使用设置中的提炼专用小模型,缺省回退当前会话模型。
 *  B2:Agent 模板 memory_write=false 时跳过(如代码审查/翻译等不写记忆的智能体)。 */
export async function maybeAutoSummarize(
  sessionId: string, providerId: string, model: string, memoryWrite: boolean,
) {
  if (!memoryWrite || summarizing.has(sessionId)) return;
  try {
    const need = await ipc.needsSummarize(sessionId);
    if (!need) return;
    summarizing.add(sessionId);
    // 提炼专用模型设置(缺省 = 当前会话模型);base_url 由后端按 provider_id 解析模板
    const settings = await ipc.getSettings().catch(() => null);
    const content = await ipc.summarizeMemory({
      session_id: sessionId,
      provider_id: settings?.summarize_provider_id || providerId,
      model: settings?.summarize_model || model,
      custom_base_url: null, custom_display_name: null,
    });
    await ipc.writeMemory(sessionId, content, 'auto_summarize');
    await useMemoryStore.getState().load(sessionId);
  } catch (e) {
    console.warn('[auto_summarize] failed:', e);
  } finally {
    summarizing.delete(sessionId);
  }
}

/** 用记忆正文 + 注入策略组装 system(学习进度块与记忆正文并列追加)。 */
export function composeSystemPrompt(
  base: string, memBody: string, strategy: MemoryStrategy, studyDigest: string | null,
): string {
  const injected = injectByStrategy(memBody, strategy);
  // 学习进度块:与记忆正文并列追加,超预算时同样受护栏约束
  const studyBlock = studyDigest
    ? `\n\n以下是用户当前的学习进度(来自学习规划板块),回答学习相关问题时请参考:\n${studyDigest}`
    : '';
  if (!injected && !studyBlock) return base;
  if (!injected) return `${base}${studyBlock}`;
  // D3.5 注入预算护栏:超限硬截断兜底(正常路径已在发送前自动压缩)
  const memText = enforceBudget(injected);
  const head = `${base}\n\n以下是关于当前会话的记忆(memory.md),请在回答时参考:\n\n${memText}`;
  return studyBlock ? `${head}${studyBlock}` : head;
}

/** D3.5:记忆预算护栏(发送前检测,超限自动压缩),返回压缩提示(无提示时 null)。 */
export async function compressMemoryIfOverBudget(
  sessionId: string,
  current: { providerId: string; model: string },
  reload: (sessionId: string) => Promise<void>,
): Promise<string | null> {
  let st: ipc.BudgetStatus;
  try {
    st = await ipc.memoryBudgetStatus(sessionId);
  } catch {
    return null;
  }
  if (!st.over) return null;
  try {
    // 提炼专用小模型优先(D3.7)
    const settings = await ipc.getSettings().catch(() => null);
    const content = await ipc.summarizeMemory({
      session_id: sessionId,
      provider_id: settings?.summarize_provider_id || current.providerId,
      model: settings?.summarize_model || current.model,
      custom_base_url: null, custom_display_name: null,
    });
    await ipc.writeMemory(sessionId, content, 'auto_compress');
    await reload(sessionId);
    const after = await ipc.memoryBudgetStatus(sessionId).catch(() => null);
    const to = after?.tokens ?? '?';
    return `记忆已自动压缩(约 ${st.tokens} → ${to} tokens,上限 ${MEMORY_BUDGET_TOKENS})`;
  } catch {
    return '记忆超出预算,自动压缩失败,本次注入将临时截断';
  }
}
