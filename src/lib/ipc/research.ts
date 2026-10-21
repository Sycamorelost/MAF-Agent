import { invoke, Channel } from '@tauri-apps/api/core';


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

/** 参考资料元数据(不返回正文)。 */
export interface ResearchReferenceInfo {
  /** 安全文件名(作为 reference_files 的取值) */
  name: string;
  /** docx / xlsx / pdf / text */
  kind: string;
  /** 提取字符数 */
  chars: number;
  /** 前 200 字预览 */
  preview: string;
}

/** 导入一份研究参考资料(前端读文件为 base64 后调用;仅支持文本类格式)。 */
export const importResearchReference = (fileName: string, dataBase64: string) =>
  invoke<ResearchReferenceInfo>('import_research_reference', {
    fileName,
    dataBase64,
  });

/** 列出本机已导入的参考资料(字符数取提取缓存长度)。 */
export const listResearchReferences = () =>
  invoke<ResearchReferenceInfo[]>('list_research_references');

/** 删除一份已导入的研究参考资料(原始文件 + 提取缓存)。 */
export const deleteResearchReference = (name: string) =>
  invoke<void>('delete_research_reference', { name });

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


