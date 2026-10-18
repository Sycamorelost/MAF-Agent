//! Tauri IPC 封装,统一类型与错误处理。

import { invoke, Channel } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';

// ============ 类型 ============

export interface SessionRow {
  id: string;
  title: string;
  created: string;
  updated: string;
  model_id: string | null;
  memory_version: number;
  /** 会话级系统提示词(空 = 默认) */
  system_prompt: string | null;
  /** 上下文滑窗:注入 LLM 的最近消息条数上限(0 = 全部) */
  max_context_messages: number;
  /** 对话内联网开关 */
  web_search_enabled: boolean;
  /** 切换模型时的记忆注入策略(D3.1):full / summary / hybrid / none */
  memory_injection_strategy: MemoryStrategy;
  /** 绑定的 Agent 模板 id(B2;null = 空白会话) */
  agent_id: string | null;
  /** 当前激活分支(F4;'main' 或 8 位 hex) */
  active_branch: string;
}

/** Agent 模板(迭代 B2,与后端 AgentTemplate 对齐)。 */
export interface AgentTemplate {
  agent_id: string;
  name: string;
  /** 图标 key,映射到 icons.tsx 的线性 SVG */
  icon: string;
  description: string;
  /** chat / doc / dev / create / ops / meta */
  category: string;
  recommended_model: string;
  /** 记忆注入策略:full / summary / none */
  memory_strategy: 'full' | 'summary' | 'none';
  system_prompt: string;
  input_hints: string[];
  /** 是否参与记忆自动提炼 */
  memory_write: boolean;
  /** 提炼写入 memory.md 的节(展示用) */
  memory_sections: string[];
  /** 变体的基座模板 id */
  base_agent: string | null;
  /** 内置模板不可删除/覆盖 */
  builtin: boolean;
}

/** 记忆注入策略(与后端 INJECTION_STRATEGIES 对齐)。 */
export type MemoryStrategy = 'full' | 'summary' | 'hybrid' | 'none';

export interface MessageRow {
  id: number;
  session_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  model_id: string | null;
  created: string;
  tokens: number;
  /** 附件元数据 JSON 数组字符串(无附件为 null) */
  attachments: string | null;
  /** 联网来源 JSON 数组字符串(assistant 消息,无联网为 null) */
  sources: string | null;
  /** 链上父消息 id(F4;分支可见历史回溯指针) */
  parent_id: number | null;
  /** 所属分支(F4;'main' 或 8 位 hex) */
  branch_id: string;
  /** 思考过程文本(V14;assistant 专用,无思考为空串) */
  reasoning: string;
}

/** 分支信息(F4,与后端 BranchInfo 对齐)。 */
export interface BranchInfo {
  branch_id: string;
  /** 用户自定义名(空 = 显示默认名) */
  name: string;
  created: string;
  /** 分叉点(main 为 null) */
  fork_parent_id: number | null;
  /** 分支自有消息数(不含共享前缀) */
  message_count: number;
  /** 首条自有消息摘要 */
  first_summary: string;
}

/** 附件元数据(messages.attachments JSON 数组的元素)。 */
export interface AttachmentMeta {
  name: string;
  path: string;
  kind: 'docx' | 'xlsx' | 'pdf' | 'text' | 'image' | 'other';
  size: number;
  /** 文本类附件的提取文本(已截断 8000 字符) */
  extracted_text: string | null;
}

export interface Frontmatter {
  conversation: string;
  id: string;
  created: string;
  models_used: string[];
  memory_version: number;
  updated: string;
  [extra: string]: unknown;
}

export interface MemoryDto {
  frontmatter: Frontmatter;
  body: string;
}

export interface VersionItem {
  id: number;
  created: string;
  note: string;
}

export interface ProviderTemplate {
  id: string;
  display_name: string;
  base_url: string;
  /** 已启用模型(按用户排序),供模型选择器使用 */
  models: string[];
  /** 全部模型(含已停用,按用户排序),仅设置页展示/启用开关使用 */
  all_models: string[];
  image_models: string[];
  docs_url: string | null;
  openai_compat: boolean;
}

export type StreamChunk =
  | { type: 'delta'; content: string }
  | { type: 'reasoning'; content: string }
  | {
      type: 'done';
      finish_reason: string | null;
      prompt_tokens: number | null;
      completion_tokens: number | null;
      /** 思考 token(仅计数无明文思考的端点如 OpenAI 官方) */
      reasoning_tokens: number | null;
    }
  | { type: 'error'; message: string };

// ============ session ============

export const createSession = (title: string, modelId: string, agentId?: string | null) =>
  invoke<{ id: string; memory_path: string }>('create_session', {
    title,
    modelId,
    agentId: agentId ?? null,
  });

export const listSessions = () => invoke<SessionRow[]>('list_sessions');

export const getSession = (id: string) => invoke<SessionRow | null>('get_session', { id });

export const renameSession = (id: string, title: string) =>
  invoke<void>('rename_session', { id, title });

export const setSessionModel = (id: string, modelId: string) =>
  invoke<void>('set_session_model', { id, modelId });

export const deleteSession = (id: string) => invoke<void>('delete_session', { id });

/** 仅当会话没有任何消息时删除(空对话默认不保留),返回是否已删。 */
export const deleteSessionIfEmpty = (id: string) =>
  invoke<boolean>('delete_session_if_empty', { id });

/** 批量清理空会话(0 条消息且超过 grace 秒未更新),返回被删 id 列表。 */
export const pruneEmptySessions = (graceSecs?: number) =>
  invoke<string[]>('prune_empty_sessions', { graceSecs: graceSecs ?? null });

export const updateSessionSettings = (
  id: string,
  systemPrompt: string | null,
  maxContextMessages: number,
) =>
  invoke<void>('update_session_settings', {
    id,
    systemPrompt,
    maxContextMessages,
  });

/** 更新会话的对话内联网开关。 */
export const setSessionWebSearch = (id: string, enabled: boolean) =>
  invoke<void>('set_session_web_search', { id, enabled });

/** 更新切换模型时的记忆注入策略(D3.1)。 */
export const setSessionMemoryStrategy = (id: string, strategy: MemoryStrategy) =>
  invoke<void>('set_session_memory_strategy', { id, strategy });

// ============ Agent 模板(B2) ============

export const listAgentTemplates = () => invoke<AgentTemplate[]>('list_agent_templates');

export const saveAgentTemplate = (agent: AgentTemplate) =>
  invoke<void>('save_agent_template', { agent });

export const deleteAgentTemplate = (agentId: string) =>
  invoke<void>('delete_agent_template', { agentId });

/** 绑定/解绑会话的 Agent(agentId=null 解绑;绑定时同步记忆注入策略 + 渲染写入 system_prompt)。 */
export const setSessionAgent = (id: string, agentId: string | null) =>
  invoke<void>('set_session_agent', { id, agentId });

/** 仅把 Agent 渲染后的 system_prompt 写入会话设置(B4 / F2,不改变绑定)。 */
export const applyAgentPrompt = (id: string, agentId: string) =>
  invoke<void>('apply_agent_prompt', { id, agentId });

// ============ AI 指令库(B3 / F1)============

/** 文本指令(与后端 TextCommand 对齐)。 */
export interface TextCommand {
  id: string;
  name: string;
  /** 图标 key,映射到 icons.tsx 的线性 SVG */
  icon: string;
  /** 指令模板;支持 {selection} / {clipboard} 变量;空 = 只带入引用 */
  prompt_template: string;
  /** 内置指令不可删除/覆盖 */
  builtin: boolean;
  /** 排序权重(小的在前) */
  sort: number;
  /** chat=对话指令面板 / both=同 chat(selection 为历史遗留值,当前无消费方) */
  scope: 'selection' | 'chat' | 'both';
}

export const listTextCommands = () => invoke<TextCommand[]>('list_text_commands');

export const saveTextCommand = (cmd: TextCommand) =>
  invoke<void>('save_text_command', { cmd });

export const deleteTextCommand = (id: string) => invoke<void>('delete_text_command', { id });

/** 渲染指令模板:selection 为划词文本(对话场景传 null),{clipboard} 由后端读取。 */
export const renderTextCommand = (id: string, selection: string | null) =>
  invoke<string>('render_text_command', { id, selection });

// ============ message ============

export interface AppendMessageInput {
  session_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  model_id?: string | null;
  tokens?: number;
  attachments?: string | null;
  sources?: string | null;
  /** 思考过程文本(V14;assistant 消息可选,缺省空串) */
  reasoning?: string | null;
}

export const appendMessage = (input: AppendMessageInput) =>
  invoke<number>('append_message', { input });

/** 列出分支可见消息(branchId 缺省 = 会话当前激活分支)。 */
export const listMessages = (sessionId: string, branchId?: string) =>
  invoke<MessageRow[]>('list_messages', { sessionId, branchId: branchId ?? null });

/** 清空当前激活分支的自有消息(其他分支不受影响)。 */
export const clearMessages = (sessionId: string) =>
  invoke<void>('clear_messages', { sessionId });

export const deleteMessage = (id: number) => invoke<void>('delete_message', { id });

/** 删除当前激活分支中 id >= fromId 的全部自有消息(放弃分支尾部),返回删除条数。 */
export const deleteMessagesFrom = (sessionId: string, fromId: number) =>
  invoke<number>('delete_messages_from', { sessionId, fromId });

export const updateMessage = (id: number, content: string) =>
  invoke<void>('update_message', { id, content });

// ============ 会话分支树(F4) ============

export const listBranches = (sessionId: string) =>
  invoke<BranchInfo[]>('list_branches', { sessionId });

/** 从指定消息处分叉并切换激活分支(旧分支完整保留),返回新 branch_id。 */
export const forkBranch = (sessionId: string, fromMessageId: number) =>
  invoke<string>('fork_branch', { sessionId, fromMessageId });

export const setActiveBranch = (sessionId: string, branchId: string) =>
  invoke<void>('set_active_branch', { sessionId, branchId });

/** 重命名分支(空名 = 恢复默认显示名)。 */
export const renameBranch = (sessionId: string, branchId: string, name: string) =>
  invoke<void>('rename_branch', { sessionId, branchId, name });

/** 删除分支(main 不可删;删除激活分支自动回退 main)。 */
export const deleteBranch = (sessionId: string, branchId: string) =>
  invoke<void>('delete_branch', { sessionId, branchId });

// ============ attachment(迭代 D1)============

/** 单附件大小上限,与后端 save_attachment 的 20MB 护栏保持一致。 */
export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024;
/** 单条消息附件数量上限,防止 base64 IPC 与请求体无限放大(M-10)。 */
export const MAX_ATTACHMENTS = 10;

/**
 * 校验待加入的附件文件:空文件 / 20MB 上限 / 数量上限。
 * existing 为当前已携带的附件数;返回首个错误信息,null 表示通过。
 */
export function validateAttachmentFiles(
  files: Array<{ name: string; size: number }>,
  existing = 0,
): string | null {
  if (existing + files.length > MAX_ATTACHMENTS) {
    return `最多携带 ${MAX_ATTACHMENTS} 个附件(当前已有 ${existing} 个)`;
  }
  for (const f of files) {
    if (f.size <= 0) return `附件「${f.name}」为空文件`;
    if (f.size > MAX_ATTACHMENT_BYTES) return `附件「${f.name}」超过 20MB 上限`;
  }
  return null;
}

/** File → data URL(base64),供 saveAttachment 使用。 */
export function readFileAsDataURL(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as string);
    r.onerror = () => reject(r.error ?? new Error('读取文件失败'));
    r.readAsDataURL(f);
  });
}

/** 保存附件:base64 → 落盘 data/attachments/{sessionId}/ → 文本类立即提取文本。 */
export const saveAttachment = (sessionId: string, fileName: string, dataBase64: string) =>
  invoke<AttachmentMeta>('save_attachment', { sessionId, fileName, dataBase64 });

/** 读取附件字节返回 data URL(渲染历史图片附件用)。 */
export const readAttachmentData = (path: string) =>
  invoke<string>('read_attachment_data', { path });

// ============ 附件管理中心(迭代 F7)============

/** 附件中心条目:messages.attachments JSON 展开 + 联表会话标题。 */
export interface AttachmentEntry {
  message_id: number;
  session_id: string;
  session_title: string;
  /** 来源消息所在分支(定位前需切换) */
  branch_id: string;
  /** 来源消息创建时间 */
  created: string;
  meta: AttachmentMeta;
}

/** 列出全部历史附件(按消息 id 倒序)。 */
export const listAllAttachments = () => invoke<AttachmentEntry[]>('list_all_attachments');

/** 把附件中心已有文件再次引入某会话(复用磁盘文件,重新生成元数据)。 */
export const insertAttachmentToSession = (sessionId: string, path: string) =>
  invoke<AttachmentMeta>('insert_attachment_to_session', { sessionId, path });

/** 删除附件:从来源消息 JSON 移除条目;全库无引用时清理磁盘文件。 */
export const deleteAttachment = (messageId: number, path: string) =>
  invoke<void>('delete_attachment', { messageId, path });

// ============ memory ============

export const readMemory = (sessionId: string) =>
  invoke<MemoryDto>('read_memory', { sessionId });

export const writeMemory = (sessionId: string, content: string, note?: string) =>
  invoke<void>('write_memory', { sessionId, content, note: note ?? null });

/** 追加文本到 memory.md 指定节(缺省「资料」,节名限 6 个标准节;note=pin_message)。 */
export const appendMemory = (sessionId: string, text: string, section?: string) =>
  invoke<void>('append_memory', { sessionId, text, section: section ?? null });

export const snapshotMemory = (sessionId: string, note?: string) =>
  invoke<string>('snapshot_memory', { sessionId, note: note ?? null });

export const listMemoryVersions = (sessionId: string) =>
  invoke<VersionItem[]>('list_memory_versions', { sessionId });

export const rollbackMemory = (sessionId: string, versionId: number) =>
  invoke<void>('rollback_memory', { sessionId, versionId });

/** 读取指定历史版本的完整快照内容(D3.3 版本 diff 用)。 */
export const readMemoryVersion = (sessionId: string, versionId: number) =>
  invoke<string>('read_memory_version', { sessionId, versionId });

/** 记忆注入预算状态(D3.5)。 */
export interface BudgetStatus {
  tokens: number;
  limit: number;
  over: boolean;
}

export const memoryBudgetStatus = (sessionId: string) =>
  invoke<BudgetStatus>('memory_budget_status', { sessionId });

export const watchMemory = (sessionId: string) =>
  invoke<void>('watch_memory', { sessionId });

export const unwatchMemory = (sessionId: string) =>
  invoke<void>('unwatch_memory', { sessionId });

export const needsSummarize = (sessionId: string) =>
  invoke<boolean>('needs_summarize', {
    sessionId,
    everyNRounds: null,
    tokenThreshold: null,
  });

export interface SummarizeMemoryInput {
  session_id: string;
  provider_id: string;
  model: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
}

/** 调 LLM 提炼记忆,返回更新后的完整 memory.md(含 frontmatter);不落盘。 */
export const summarizeMemory = (input: SummarizeMemoryInput) =>
  invoke<string>('summarize_memory', { input });

// ============ 应用设置(D3.7 / D9)============

export interface AppSettings {
  /** 提炼专用 provider_id(null = 用当前会话模型) */
  summarize_provider_id: string | null;
  /** 提炼专用模型(null = 用当前会话模型) */
  summarize_model: string | null;
  /** 划词提问:任意应用中连按两次 Ctrl+C 唤起主窗口 */
  selection_ask_enabled: boolean;
  /** 主窗口最小化时隐藏到托盘 */
  minimize_to_tray: boolean;
}

export const getSettings = () => invoke<AppSettings>('get_settings');

/** 保存设置并广播 settings_changed。 */
export const saveSettings = async (settings: AppSettings) => {
  await invoke<void>('save_settings', { settings });
  await emit('settings_changed', settings);
};

// ============ 数据目录(可自定义)============

export interface DataDirInfo {
  /** 当前数据根目录(绝对路径) */
  path: string;
  /** 引导配置文件路径(exe 旁 config.json,不可写时回退 %APPDATA%) */
  config_path: string;
  /** 是否已由用户显式选定过 */
  configured: boolean;
  /** 是否采用了历史数据目录(老版本升级) */
  legacy_adopted: boolean;
  /** 当前目录是否已有数据库文件 */
  has_data: boolean;
  /** 上次迁移失败的原因 */
  migration_error: string | null;
  /** 是否为开发构建(开发期不弹首次启动引导) */
  is_dev: boolean;
}

export interface SetDataDirResult {
  /** 是否需要重启应用才能生效 */
  restart_required: boolean;
  /** 待迁移的旧目录(无则 null) */
  old_dir: string | null;
}

export const getDataDir = () => invoke<DataDirInfo>('get_data_dir');

/** 弹出原生文件夹选择器;用户取消返回 null。 */
export const pickDataDir = () => invoke<string | null>('pick_data_dir');

/** 切换数据目录;migrate=true 时把旧目录数据搬到新目录(下次启动执行)。 */
export const setDataDir = (path: string, migrate: boolean) =>
  invoke<SetDataDirResult>('set_data_dir', { path, migrate });

/** 在资源管理器中打开当前数据目录。 */
export const openDataDir = () => invoke<void>('open_data_dir');

/** 重启应用(目录切换与迁移需要新进程生效)。 */
export const restartApp = () => invoke<void>('restart_app');

// ============ provider ============

export const listProviderTemplates = () =>
  invoke<ProviderTemplate[]>('list_provider_templates');

export const saveApiKey = (providerId: string, apiKey: string) =>
  invoke<void>('save_api_key', { providerId, apiKey });

export const hasApiKey = (providerId: string) =>
  invoke<boolean>('has_api_key', { providerId });

export const deleteApiKey = (providerId: string) =>
  invoke<void>('delete_api_key', { providerId });

export const listStoredProviderIds = () =>
  invoke<string[]>('list_stored_provider_ids');

// ---- API 流转:多 Key 槽位 + 轮换配置 + 用量统计 ----

/** Key 槽位视图(不含明文 Key,仅掩码)。 */
export interface KeySlotView {
  slot: number;
  label: string;
  enabled: boolean;
  /** 权重(>=1,相对流量份额) */
  weight: number;
  /** 优先级(越大越先使用) */
  priority: number;
  /** 掩码,如 sk-a****1234 */
  masked: string | null;
  /** 剩余冷却秒数(0 = 可用) */
  cooldown_remaining: number;
}

/** 单个 Key 槽位的用户元数据。 */
export interface KeySlotMeta {
  slot: number;
  label: string;
  enabled: boolean;
  weight: number;
  priority: number;
}

/** 每 Provider 的轮换配置。 */
export interface RotationConfig {
  enabled: boolean;
  strategy: string;
  cooldown_secs: number;
  supports_stream_options: boolean;
  keys: KeySlotMeta[];
}

export type RotationConfigs = Record<string, RotationConfig>;

export interface UsageTotals {
  requests: number;
  tokens: number;
  failed: number;
  rate_limited: number;
  avg_first_token_ms: number | null;
  /** 总成本(微元,1e-6 元) */
  cost_micro: number;
}

export interface UsageGroup {
  key: string;
  name: string;
  slot_id: number | null;
  cooling: boolean;
  requests: number;
  tokens: number;
  failed: number;
  rate_limited: number;
  avg_first_token_ms: number | null;
  /** 该分组成本(微元,1e-6 元) */
  cost_micro: number;
}

export interface UsageSummary {
  totals: UsageTotals;
  groups: UsageGroup[];
}

/** 热力图一格:星期(0=周日)× 小时。 */
export interface UsageHour {
  dow: number;
  hour: number;
  requests: number;
  tokens: number;
}

export interface UsageEntry {
  id: number;
  provider_id: string;
  model_id: string;
  slot_id: number;
  slot_label: string | null;
  key_masked: string | null;
  status: string;
  error_kind: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  /** 思考 token(V14;思考开启的请求才有) */
  reasoning_tokens: number | null;
  first_token_ms: number | null;
  total_ms: number | null;
  created_at: number;
  /** 会话绑定的助手 id(未绑定为 null) */
  agent_id: string | null;
  agent_name: string | null;
  /** 本次请求成本(微元,1e-6 元);null = 无定价信息 */
  cost_micro: number | null;
  cost_source: string | null;
}

export const listProviderKeys = (providerId: string) =>
  invoke<KeySlotView[]>('list_provider_keys', { providerId });

export const saveProviderKey = (
  providerId: string,
  apiKey: string,
  label?: string,
  slot?: number,
) =>
  invoke<number>('save_provider_key', {
    providerId,
    apiKey,
    label: label ?? null,
    slot: slot ?? null,
  });

export const updateKeySlot = (
  providerId: string,
  slot: number,
  label: string,
  enabled: boolean,
  weight?: number,
  priority?: number,
) =>
  invoke<void>('update_key_slot', {
    providerId,
    slot,
    label,
    enabled,
    weight: weight ?? null,
    priority: priority ?? null,
  });

export const deleteProviderKey = (providerId: string, slot: number) =>
  invoke<void>('delete_provider_key', { providerId, slot });

export const getRotationConfigs = () =>
  invoke<RotationConfigs>('get_rotation_configs');

export const setProviderRotation = (
  providerId: string,
  enabled: boolean,
  cooldownSecs: number,
  supportsStreamOptions: boolean,
) =>
  invoke<void>('set_provider_rotation', {
    providerId,
    enabled,
    cooldownSecs,
    supportsStreamOptions,
  });

export const usageSummary = (dimension: 'model' | 'provider' | 'key' | 'agent', days: number) =>
  invoke<UsageSummary>('usage_summary', { dimension, days });

export const usageEntries = (limit: number) =>
  invoke<UsageEntry[]>('usage_entries', { limit });

export const usageKeyHealth = (providerId: string, days: number) =>
  invoke<UsageGroup[]>('usage_key_health', { providerId, days });

/** 热力图数据:近 days 天按「星期 × 小时」分桶。 */
export const usageHourly = (days: number) => invoke<UsageHour[]>('usage_hourly', { days });

/** 从厂家拉取模型列表(GET /models),缓存并返回完整列表。 */
export const fetchProviderModels = (providerId: string) =>
  invoke<string[]>('fetch_provider_models', { providerId });

// ---- 单模型配置(迭代 D8,与后端 ModelOverride 对齐)----

/** 单模型覆盖项;null = 不覆盖,走默认行为。 */
export interface ModelOverride {
  enabled: boolean | null;
  display_name: string | null;
  order: number | null;
  temperature: number | null;
  max_tokens: number | null;
  top_p: number | null;
  memory_injection: MemoryStrategy | null;
  /** 默认思考强度(v1.9.0;light/medium/high/ultra,null = 跟随全局默认 medium) */
  default_reasoning: string | null;
}

/** provider_id -> model_id -> 覆盖项 */
export type ProviderModelConfig = Record<string, Record<string, ModelOverride>>;

export const EMPTY_MODEL_OVERRIDE: ModelOverride = {
  enabled: null,
  display_name: null,
  order: null,
  temperature: null,
  max_tokens: null,
  top_p: null,
  memory_injection: null,
  default_reasoning: null,
};

export const getProviderModelConfig = () =>
  invoke<ProviderModelConfig>('get_provider_model_config');

export const saveProviderModelConfig = (config: ProviderModelConfig) =>
  invoke<void>('save_provider_model_config', { config });

// ---- 模型思考能力(v1.9.0 工作块 I,与后端 reasoning::ReasoningCapability 对齐)----

/** 一个模型的思考能力声明(手动修正 > 内置规则表推断 > 不支持)。 */
export interface ReasoningCapability {
  /** 是否支持思考控制(false = 控件整体隐藏) */
  enabled: boolean;
  /** 是否允许关闭(false = 不可关模型如 Kimi K3 / GLM-5.3,UI 锁定为开) */
  can_disable: boolean;
  /** 是否有档位差异(false = 仅开关,如 Kimi K2.6,四档控件禁用) */
  levels: boolean;
}

export const getReasoningCapability = (providerId: string, model: string) =>
  invoke<ReasoningCapability>('get_reasoning_capability', { providerId, model });

/** 手动修正模型思考能力(写入模型缓存;cap=null 清除修正回到内置推断)。 */
export const saveReasoningOverride = (
  providerId: string,
  model: string,
  cap: ReasoningCapability | null,
) => invoke<void>('save_reasoning_override', { providerId, model, cap });

/** 读取某模型的手动思考能力修正原始值(未修正返回 null;设置页区分「自动推断/手动修正」用)。 */
export const getReasoningOverride = (providerId: string, model: string) =>
  invoke<ReasoningCapability | null>('get_reasoning_override', { providerId, model });

// ---- 模型卡(迭代 D7.1,与后端 ModelCard 对齐)----

export interface ModelPricing {
  input_cache_hit: number | null;
  input_cache_miss: number | null;
  input: number | null;
  output: number | null;
  per_image: number | null;
  per_char: number | null;
  note: string | null;
}

export interface MemoryInjectionSpec {
  strategy: string;
  template: string | null;
}

export interface ModelDefaults {
  temperature: number | null;
  top_p: number | null;
  system_prompt: string | null;
}

export interface ModelCard {
  id: string;
  display_name: string;
  provider: string;
  official_model_id: string;
  context_window: number | null;
  max_output: number | null;
  cost_tier: 'free' | 'budget' | 'standard' | 'premium' | 'ultra' | string;
  tags: string[];
  pricing: ModelPricing | null;
  defaults: ModelDefaults | null;
  memory_injection: MemoryInjectionSpec | null;
}

/** 内置 13 张模型卡 + 厂家拉取派生卡 + data/model_cards.json 用户覆盖/追加。 */
export const listModelCards = () => invoke<ModelCard[]>('list_model_cards');

/** 手动新增/覆盖一张模型卡(价格/上下文等元数据)。 */
export const saveModelCard = (card: ModelCard) =>
  invoke<void>('save_model_card', { card });

/** 厂家派生卡 id 规则(与后端 vendor_card_id 保持一致)。 */
export const vendorCardId = (providerId: string, modelId: string) =>
  `vendor-${providerId}-${modelId.replace(/[^A-Za-z0-9._-]/g, '-')}`;

/** 成本档位中文名。 */
export const COST_TIER_LABEL: Record<string, string> = {
  free: '免费',
  budget: '经济',
  standard: '标准',
  premium: '高端',
  ultra: '旗舰',
};

/** 格式化 token 数为紧凑文本(1M / 200K / 32K)。 */
export function formatTokens(n: number | null | undefined): string {
  if (n == null) return '-';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(n % 1_000 === 0 ? 0 : 1)}K`;
  return String(n);
}

// ============ 服务商模板增删改(用户拥有完全权限) ============

/** 新增或更新模板(id 为空时后端自动生成),返回落盘后的模板。 */
export const saveProviderTemplate = (template: ProviderTemplate) =>
  invoke<ProviderTemplate>('save_provider_template', { template });

/** 删除模板(内置/用户新增一视同仁),同时清理其 Key、单模型配置与模型缓存。 */
export const deleteProviderTemplate = (providerId: string) =>
  invoke<void>('delete_provider_template', { providerId });

/** 补回被删除的内置模板(仅补缺失项)。 */
export const restoreBuiltinTemplates = () => invoke<void>('restore_builtin_templates');

// ============ chat_stream ============

export interface ChatStreamInput {
  provider_id: string;
  model: string;
  messages: Array<{ role: string; content: string; images?: string[] }>;
  temperature?: number | null;
  max_tokens?: number | null;
  top_p?: number | null;
  /** 思考档位(v1.9.0):off / light / medium / high / ultra;缺省 = 后端按能力与默认档合并 */
  reasoning_effort?: string | null;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
  /** 会话 id(用于按助手归因;可选) */
  session_id?: string | null;
  /** 前端生成的取消句柄;cancel() 时凭它通知后端中止 HTTP 请求(M-2) */
  cancel_id?: string;
}

/** 通知后端中止指定的流式请求(chat / translate)。 */
export const cancelStream = (cancelId: string) =>
  invoke<void>('cancel_stream', { cancelId }).catch(() => {});

/**
 * 流式对话。返回一个 { done, cancel } 句柄:
 *   - done:Promise<StreamChunk & {type:'done'}>,结束后 resolve
 *   - 通过 onChunk 回调收每个 chunk
 *   - cancel():本地停止渲染 + 通知后端中断请求(不再继续计费,M-2)
 */
export function chatStream(
  input: ChatStreamInput,
  onChunk: (chunk: StreamChunk) => void,
): { done: Promise<void>; cancel: () => void } {
  const channel = new Channel<StreamChunk>();
  let cancelled = false;
  let resolveDone!: () => void;
  let rejectDone!: (e: unknown) => void;
  const done = new Promise<void>((res, rej) => {
    resolveDone = res;
    rejectDone = rej;
  });

  channel.onmessage = (chunk) => {
    if (cancelled) return;
    onChunk(chunk);
    if (chunk.type === 'done') {
      resolveDone();
    } else if (chunk.type === 'error') {
      rejectDone(new Error(chunk.message));
    }
  };

  const cancelId = input.cancel_id ?? crypto.randomUUID();
  void invoke<void>('chat_stream', { input: { ...input, cancel_id: cancelId }, channel }).catch(
    (e) => {
      if (!cancelled) rejectDone(e);
    },
  );

  return {
    done,
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      cancelStream(cancelId);
      resolveDone();
    },
  };
}

// ============ events ============

/// 监听 memory.md 外部改动。返回 unlisten。
export function onMemoryChanged(
  cb: (payload: { session_id: string; kind: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ session_id: string; kind: string }>('memory_changed', (e) => cb(e.payload));
}

// ============ research(Phase 2 深度研究)============

export interface SourceItem {
  id: number;
  title: string;
  url: string;
  snippet: string;
  engine: string;
  published: string | null;
}

export type ResearchEvent =
  | { type: 'round_start'; round: number; max_rounds: number; stage?: number }
  | { type: 'thought'; round: number; content: string; stage?: number }
  | { type: 'queries'; round: number; queries: string[]; stage?: number }
  | { type: 'searching'; round: number; query: string; stage?: number }
  | { type: 'sources'; round: number; sources: SourceItem[]; stage?: number }
  | { type: 'observation'; round: number; content: string; stage?: number }
  | { type: 'paused'; round: number; stage?: number }
  | { type: 'resumed'; round: number; stage?: number }
  | { type: 'stage_start'; stage: number; name: string; kind: string }
  | { type: 'stage_done'; stage: number; summary: string; sources_count: number; partial?: boolean }
  | { type: 'stage_skipped'; stage: number; reason: string }
  | { type: 'angle_gap'; stage: number; missing: string[] }
  | { type: 'checkpoint'; stage: number; next: string }
  | { type: 'report_delta'; content: string }
  | { type: 'done'; report: string }
  | { type: 'cancelled' }
  | { type: 'error'; message: string };

/** 阶段内约束(与后端 StageConstraints 对应;null/默认 = 不施加约束)。 */
export interface StageConstraints {
  /** 本阶段 Token 上限(null = 不限) */
  stage_token_budget: number | null;
  /** 本阶段检索次数上限 */
  stage_max_searches: number | null;
  /** 本阶段新增资料上限 */
  stage_max_sources: number | null;
  /** 资料数下限:未达标自动补轮(最多 2 轮)并标注缺口 */
  min_sources: number | null;
  /** 必覆盖角度:结束校验,缺失提醒不阻断 */
  must_cover_angles: string[];
  /** 阶段域名白名单(source_mode=whitelist 且未覆盖 whitelist_domains 时生效) */
  include_domains: string[];
  /** 域名黑名单:检索结果过滤 */
  exclude_domains: string[];
  /** 查询词语言倾向:auto / zh / en */
  query_language: string;
  /** 允许模型判定「资料足够」提前 finish(false = 跑满轮次) */
  early_stop_on_enough: boolean;
  /** 决策上下文注入前阶段摘要 */
  inject_prev_summaries: boolean;
  /** 决策上下文可见前阶段资料明细 */
  inherit_sources: boolean;
}

export const DEFAULT_STAGE_CONSTRAINTS: StageConstraints = {
  stage_token_budget: null,
  stage_max_searches: null,
  stage_max_sources: null,
  min_sources: null,
  must_cover_angles: [],
  include_domains: [],
  exclude_domains: [],
  query_language: 'auto',
  early_stop_on_enough: true,
  inject_prev_summaries: true,
  inherit_sources: false,
};

/** 阶段配置(与后端 StageConfig 对应;null = 跟随全局)。 */
export interface StageConfig {
  id: string;
  name: string;
  goal: string;
  /** research = 检索阶段 / synthesis = 纯撰写阶段 */
  kind: string;
  enabled: boolean;
  max_rounds: number | null;
  queries_per_round: number | null;
  source_mode: string | null;
  whitelist_domains: string[] | null;
  read_length: string | null;
  reflect: string | null;
  decide_prompt: string | null;
  summary_prompt: string | null;
  constraints: StageConstraints;
}

let stageSeq = 0;
/** 新建阶段(前端生成唯一 id)。 */
export function newStage(name: string, kind: string): StageConfig {
  stageSeq += 1;
  return {
    id: `s${Date.now().toString(36)}${stageSeq}`,
    name,
    goal: '',
    kind,
    enabled: true,
    max_rounds: null,
    queries_per_round: null,
    source_mode: null,
    whitelist_domains: null,
    read_length: null,
    reflect: null,
    decide_prompt: null,
    summary_prompt: null,
    constraints: { ...DEFAULT_STAGE_CONSTRAINTS, must_cover_angles: [], include_domains: [], exclude_domains: [] },
  };
}

/** 内置「开发策略研究」四阶段方案(配置页切到专项模式时的默认编排)。 */
export function builtinStages(): StageConfig[] {
  const s1 = newStage('背景与现状摸底', 'research');
  s1.goal = '收集主题相关的基础定义、发展脉络与现状数据,建立背景认知';
  s1.max_rounds = 4;
  const s2 = newStage('方案与竞品对比', 'research');
  s2.goal = '收集主流方案与竞品资料,提取各自优劣、适用场景与可对比维度';
  s2.max_rounds = 6;
  const s3 = newStage('关键技术深挖', 'research');
  s3.goal = '针对前两阶段发现的关键技术点深入检索,获取实现细节与数据支撑';
  const s4 = newStage('策略撰写', 'synthesis');
  s4.goal = '基于全部资料与阶段摘要,产出结构化对比分析与可执行的策略建议';
  return [s1, s2, s3, s4];
}

/** 深度研究可配置参数(迭代 D4,与后端 ResearchParams 对应)。 */
export interface ResearchParams {
  /** 研究深度:最大轮次(1-25) */
  max_rounds: number;
  /** 每轮查询数(1-10) */
  queries_per_round: number;
  /** 信息源:web / academic / whitelist */
  source_mode: string;
  /** 白名单域名(source_mode=whitelist 时生效) */
  whitelist_domains: string[];
  /** 单源阅读长度:digest / full */
  read_length: string;
  /** 反思策略:every / every3 / final */
  reflect: string;
  /** 最大检索次数(0 = 不限) */
  max_searches: number;
  /** Token 预算上限(0 = 不限) */
  token_budget: number;
  /** 输出语言:zh / en */
  output_lang: string;
  /** 研究模式:legacy(快速)/ staged(专项) */
  mode: string;
  /** 专项模式阶段编排 */
  stages: StageConfig[];
  /** 检查点模式:每阶段结束暂停等确认 */
  checkpoint: boolean;
  /** 自定义终稿报告模板(null = 内置/全局模板) */
  report_prompt: string | null;
  /** 全局总时长上限·分钟(0 = 不限) */
  max_duration_minutes: number;
  /** 全局资料条数上限(0 = 不限) */
  max_sources: number;
  /** 同轮查询并发数 1-5(默认 1) */
  concurrent_queries: number;
  /** 单查询失败策略:skip / abort_stage / abort_research */
  fail_policy: string;
  /** URL 去重范围:global 全程 / stage 仅阶段内 */
  dedup_scope: string;
  /** 科研增强(v1.9.0):注入内置研究方法论技能(默认开) */
  skill_enhance: boolean;
  /** 深度审计:终稿追加引用与统计审计附录(默认关) */
  skill_audit: boolean;
  /** AI 概念示意草稿(默认关) */
  skill_image_draft: boolean;
  /** 草稿生图 Provider id */
  skill_image_provider: string | null;
  /** 草稿生图模型名 */
  skill_image_model: string | null;
}

export const DEFAULT_RESEARCH_PARAMS: ResearchParams = {
  max_rounds: 8,
  queries_per_round: 4,
  source_mode: 'web',
  whitelist_domains: [],
  read_length: 'digest',
  reflect: 'every',
  max_searches: 0,
  token_budget: 120000,
  output_lang: 'zh',
  mode: 'legacy',
  stages: [],
  checkpoint: false,
  report_prompt: null,
  max_duration_minutes: 0,
  max_sources: 0,
  concurrent_queries: 1,
  fail_policy: 'skip',
  dedup_scope: 'global',
  skill_enhance: true,
  skill_audit: false,
  skill_image_draft: false,
  skill_image_provider: null,
  skill_image_model: null,
};

/** 研究方案(参数 + 阶段编排 + 提示词 + 约束;内置方案不落盘)。 */
export interface Blueprint {
  name: string;
  desc: string;
  /** true = 内置方案(不可覆盖/删除) */
  builtin: boolean;
  params: ResearchParams;
  created: string;
}

export interface ResearchRow {
  id: string;
  topic: string;
  status: 'running' | 'paused' | 'done' | 'error' | 'cancelled';
  rounds: number;
  max_rounds: number;
  model_id: string | null;
  error: string | null;
  created: string;
  updated: string;
  /** 研究模式:legacy / staged */
  mode: string;
  /** 当前/最后执行的阶段序号 */
  current_stage: number;
  blueprint_name: string | null;
  /** 已完成阶段数(M4:落库回写,列表零文件 IO;非 staged 恒 0) */
  stages_done: number;
  /** 启用阶段总数(M4:启动时落库;非 staged 恒 0) */
  stages_total: number;
}

/** 阶段运行期状态(stages.json 快照)。 */
export interface StageState {
  id: string;
  name: string;
  kind: string;
  enabled: boolean;
  /** pending / running / done / skipped */
  status: string;
  summary: string;
  sources_count: number;
  rounds: number;
  /** M2:true = 因失败策略提前中止,摘要为部分结果 */
  partial?: boolean;
}

export interface StagesFile {
  mode: string;
  stages: StageState[];
  updated: string;
  /** H1:完整阶段配置快照(检查点「调整下阶段」的数据源) */
  configs?: StageConfig[];
}

export interface ResearchDetail {
  session: ResearchRow;
  trail: ResearchEvent[];
  report: string | null;
  /** 专项模式阶段快照(legacy 为 null) */
  stages: StagesFile | null;
}

export interface SearchConfigDto {
  engine: string;
  base_url: string;
  has_api_key: boolean;
}

export const getSearchConfig = () => invoke<SearchConfigDto>('get_search_config');

export const saveSearchConfig = (input: {
  engine: string;
  base_url: string;
  api_key?: string | null;
}) => invoke<void>('save_search_config', { input });

export const searchTest = (query: string) => invoke<SourceItem[]>('search_test', { query });

export const researchList = () => invoke<ResearchRow[]>('research_list');

export const researchGet = (id: string) => invoke<ResearchDetail>('research_get', { id });

export const researchDelete = (id: string) => invoke<void>('research_delete', { id });

export const researchCancel = (id: string) => invoke<void>('research_cancel', { id });

export const researchPause = (id: string) => invoke<void>('research_pause', { id });

export const researchResume = (id: string) => invoke<void>('research_resume', { id });

// ---- 科研增强 + 报告导出(v1.9.0 工作块 II) ----

/** 内置科研方法论技能文档(健康检查用)。 */
export interface SkillDoc {
  /** 相对 resources/skills 的路径 */
  file: string;
  bytes: number;
  ok: boolean;
}

export const researchListSkills = () => invoke<SkillDoc[]>('research_list_skills');

/** 后端 H-2:目标已存在但未确认覆盖时返回的错误码。 */
export const EXPORT_TARGET_EXISTS = 'TARGET_EXISTS';

/** 导出报告 Markdown(后端同目录复制 figures/ 资源夹),返回实际写入路径。
 *  token 为 pickSavePath 签发的路径令牌(N-6:后端只认令牌,不认裸路径);
 *  overwrite=false 且目标已存在时 reject(EXPORT_TARGET_EXISTS),确认后带 true 重试。 */
export const researchExportMarkdown = (id: string, token: string, overwrite: boolean) =>
  invoke<string>('research_export_markdown', { id, token, overwrite });

/** 导出自包含 HTML(前端传已渲染 DOM,后端内嵌 figures 为 base64),返回实际写入路径。 */
export const researchExportHtml = (
  id: string,
  html: string,
  token: string,
  overwrite: boolean,
) => invoke<string>('research_export_html', { id, html, token, overwrite });

/** 读取研究图示为 data URL(报告内联预览;仅 figures 目录内图片)。 */
export const researchFigureData = (id: string, name: string) =>
  invoke<string>('research_figure_data', { id, name });

/** 原生「另存为」对话框结果。existed=选择时同名文件已存在(系统对话框已完成覆盖确认)。 */
export interface SavePathPick {
  path: string;
  existed: boolean;
  /** N-6:后端签发的路径令牌,导出命令凭令牌写盘(渲染层无法伪造任意路径)。 */
  token: string;
}

/** 原生「另存为」对话框(报告导出);取消返回 null。 */
export const pickSavePath = (
  title: string,
  defaultName: string,
  filterName: string,
  filterExt: string,
) =>
  invoke<SavePathPick | null>('pick_save_path', {
    title,
    defaultName,
    filterName,
    filterExt,
  });

/** 检查点确认继续(专项研究阶段间检查点操作条)。 */
export const researchStageContinue = (id: string) =>
  invoke<void>('research_stage_continue', { id });

/** H1:检查点「调整下阶段」— 写回最新阶段编排,引擎在下一阶段起点热读生效。 */
export const researchUpdateStages = (id: string, stages: StageConfig[]) =>
  invoke<void>('research_update_stages', { id, stages });

/** 方案库:列出全部方案(内置 + 用户)。 */
export const blueprintList = () => invoke<Blueprint[]>('blueprint_list');

/** 方案库:保存用户方案(同名覆盖;内置名受保护)。 */
export const blueprintSave = (name: string, desc: string, params: ResearchParams) =>
  invoke<Blueprint[]>('blueprint_save', { name, desc, params });

/** 方案库:删除用户方案(内置名受保护)。 */
export const blueprintDelete = (name: string) =>
  invoke<Blueprint[]>('blueprint_delete', { name });

/** 研究提示词模板(专项研究:全局决策提示 decide / 终稿报告模板 report)。 */
export interface ResearchTemplateDto {
  key: string;
  name: string;
  /** true = 当前生效为内置模板(无自定义覆盖) */
  builtin: boolean;
  /** 生效内容(自定义优先,否则内置) */
  content: string;
  /** 支持的占位符白名单(编辑器校验用) */
  placeholders: string[];
}

export const researchTemplateList = () => invoke<ResearchTemplateDto[]>('research_template_list');

export const researchTemplateSave = (key: string, content: string) =>
  invoke<ResearchTemplateDto[]>('research_template_save', { key, content });

export const researchTemplateReset = (key: string) =>
  invoke<ResearchTemplateDto[]>('research_template_reset', { key });

export interface ResearchStartInput {
  topic: string;
  provider_id: string;
  model: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
  params?: Partial<ResearchParams>;
  /** 来源研究方案名(方案库应用后随启动落库) */
  blueprint_name?: string | null;
}

/**
 * 启动深度研究。返回 { id, done, cancel }:
 *   - id:研究 id 的 Promise(命令返回后 resolve)
 *   - done:收到 done / cancelled 时 resolve,error 时 reject
 *   - cancel:等 id 就绪后调 research_cancel 终止运行
 */
export function researchStart(
  input: ResearchStartInput,
  onEvent: (ev: ResearchEvent) => void,
): { id: Promise<string>; done: Promise<void>; cancel: () => void } {
  const channel = new Channel<ResearchEvent>();
  let settled = false;
  let resolveDone!: () => void;
  let rejectDone!: (e: unknown) => void;
  const done = new Promise<void>((res, rej) => {
    resolveDone = res;
    rejectDone = rej;
  });

  channel.onmessage = (ev) => {
    onEvent(ev);
    if (ev.type === 'done' || ev.type === 'cancelled') {
      settled = true;
      resolveDone();
    } else if (ev.type === 'error') {
      settled = true;
      rejectDone(new Error(ev.message));
    }
  };

  const id = invoke<string>('research_start', { input, channel });
  id.catch((e) => {
    if (!settled) {
      settled = true;
      rejectDone(e);
    }
  });

  return {
    id,
    done,
    cancel: () => {
      id.then((rid) => researchCancel(rid).catch(() => {})).catch(() => {});
    },
  };
}

// ============ 全局搜索 ============

export interface SessionHit {
  id: string;
  title: string;
  updated: string;
}

export interface MessageHit {
  session_id: string;
  session_title: string;
  message_id: number;
  role: string;
  snippet: string;
  created: string;
  /** 消息所属分支(F4;跳转时若与会话激活分支不同,先切换再定位) */
  branch_id: string;
}

export interface MemoryHit {
  session_id: string;
  session_title: string;
  snippet: string;
}

/** 知识库命中(迭代 D8)。 */
export interface NoteHit {
  id: string;
  title: string;
  snippet: string;
  updated: string;
}

export interface SearchResults {
  sessions: SessionHit[];
  messages: MessageHit[];
  memory: MemoryHit[];
  notes: NoteHit[];
}

export const searchAll = (query: string) => invoke<SearchResults>('search_all', { query });

// ============ 知识库 / 笔记库(迭代 D8) ============

export interface NoteMeta {
  id: string;
  title: string;
  tags: string[];
  created: string;
  updated: string;
  /** 正文摘要(列表预览) */
  excerpt: string;
}

export interface NoteDoc {
  id: string;
  title: string;
  tags: string[];
  created: string;
  updated: string;
  body: string;
}

export const notesList = () => invoke<NoteMeta[]>('notes_list');

export const notesGet = (id: string) => invoke<NoteDoc>('notes_get', { id });

/** 新建/保存笔记(id=null 表示新建)。 */
export const notesSave = (id: string | null, title: string, tags: string[], body: string) =>
  invoke<NoteDoc>('notes_save', { id, title, tags, body });

export const notesDelete = (id: string) => invoke<void>('notes_delete', { id });

// ============ AI 绘画 ============

export interface GenerateImageInput {
  provider_id: string;
  model: string;
  prompt: string;
  /** 负面提示(后端合并进 prompt) */
  negative?: string | null;
  /** 风格英文关键词(后端合并进 prompt) */
  style_prompt?: string | null;
  /** 风格标签名(仅 meta 留档) */
  style?: string | null;
  size?: string | null;
  n?: number | null;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
}

export interface ImageResultItem {
  /** 落盘绝对路径(data/images/ 下) */
  path: string;
  /** base64 data URI,直接渲染 */
  data_url: string;
  revised_prompt: string | null;
}

/** 生成参数留档(与 png 同名同目录的 meta.json)。 */
export interface ImageMeta {
  prompt: string;
  negative: string | null;
  style: string | null;
  size: string;
  model: string;
  provider_id: string;
  seed: number | null;
  guidance: number | null;
  created: string;
}

/** 历史作品条目:缩略图 + meta。 */
export interface ImageListItem {
  path: string;
  /** 256px JPEG 缩略图 data URL */
  thumb: string;
  meta: ImageMeta | null;
}

export const generateImage = (input: GenerateImageInput) =>
  invoke<ImageResultItem[]>('generate_image', { input });

export const listImages = () => invoke<ImageListItem[]>('list_images');

export const readImage = (path: string) => invoke<string>('read_image', { path });

export const imageToSession = (imagePath: string, sessionId: string) =>
  invoke<AttachmentMeta>('image_to_session', { imagePath, sessionId });

// ============ AI 翻译 ============

export interface TranslateStreamInput {
  provider_id: string;
  model: string;
  text: string;
  /** 源语言英文名;空/null = 自动检测 */
  source_lang?: string | null;
  /** 目标语言英文名 */
  target_lang: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
  /** 前端生成的取消句柄;cancel() 时通知后端中止请求(M-2) */
  cancel_id?: string;
}

// ---- 术语表(迭代 D6.1)----

export interface GlossaryEntry {
  src: string;
  dst: string;
  note?: string | null;
}

export const glossaryList = () => invoke<GlossaryEntry[]>('glossary_list');

export const glossarySave = (entries: GlossaryEntry[]) =>
  invoke<void>('glossary_save', { entries });

// ---- 翻译历史(迭代 D6.2)----

export interface TranslationRecord {
  /** 落盘文件名(list 返回时携带;保存时忽略) */
  file_name?: string;
  created: string;
  provider_id: string;
  model: string;
  source_lang: string | null;
  target_lang: string;
  source_text: string;
  result_text: string;
}

export const saveTranslation = (rec: TranslationRecord) =>
  invoke<string>('save_translation', { rec });

export const listTranslations = () => invoke<TranslationRecord[]>('list_translations');

export const deleteTranslation = (fileName: string) =>
  invoke<void>('delete_translation', { fileName });

/** 流式翻译。用法同 chatStream:onChunk 收增量,done 完成时 resolve。 */
export function translateStream(
  input: TranslateStreamInput,
  onChunk: (chunk: StreamChunk) => void,
): { done: Promise<void>; cancel: () => void } {
  const channel = new Channel<StreamChunk>();
  let cancelled = false;
  let resolveDone!: () => void;
  let rejectDone!: (e: unknown) => void;
  const done = new Promise<void>((res, rej) => {
    resolveDone = res;
    rejectDone = rej;
  });

  channel.onmessage = (chunk) => {
    if (cancelled) return;
    onChunk(chunk);
    if (chunk.type === 'done') {
      resolveDone();
    } else if (chunk.type === 'error') {
      rejectDone(new Error(chunk.message));
    }
  };

  const cancelId = input.cancel_id ?? crypto.randomUUID();
  void invoke<void>('translate_stream', {
    input: { ...input, cancel_id: cancelId },
    channel,
  }).catch((e) => {
    if (!cancelled) rejectDone(e);
  });

  return {
    done,
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      cancelStream(cancelId);
      resolveDone();
    },
  };
}

// ============ 教程卡(迭代 D7.2) ============

/** 一张教程卡:tutorials/*.md 的 front-matter + markdown 正文。 */
export interface TutorialDoc {
  id: string;
  /** 所属页面:chat / research / draw / translate */
  page: string;
  title: string;
  summary: string;
  order: number;
  content: string;
}

export const listTutorials = (page: string) => invoke<TutorialDoc[]>('list_tutorials', { page });
