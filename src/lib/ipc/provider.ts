import { invoke, Channel } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { MemoryStrategy, ProviderTemplate, StreamChunk } from './types';

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



