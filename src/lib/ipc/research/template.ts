import { invoke } from '@tauri-apps/api/core';

// ============ 阶段编排 / 研究方案 / 提示词模板 ============

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
  /** 自由结构:不用报告模板,AI 自行决定报告章节结构 */
  free_structure: boolean;
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
  /** 用户导入的参考资料(仅安全文件名,落盘于 data/research/references/) */
  reference_files: string[];
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
  free_structure: false,
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
  reference_files: [],
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
