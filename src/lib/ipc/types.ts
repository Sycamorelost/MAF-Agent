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
  // —— v2 字段(R3,全部可选,旧数据兼容) ——
  /** 开场白:空会话首条展示(空 = 不展示) */
  opening_message?: string;
  /** 输入框追问建议(chips) */
  suggestions?: string[];
  /** 工具策略:null=继承全局;[]=禁用工具;[名单]=仅允许名单内工具 */
  tool_policy?: string[] | null;
  /** 思考档位偏好:'' = 跟随用户设置;off / low / medium / high */
  thinking_level?: string;
  /** 采样温度偏好(null = 跟随模型默认) */
  temperature?: number | null;
  /** 附件文档管线:inline / resident / off(空 = inline) */
  doc_pipeline?: string;
  /** 触发词(Skill 层,Phase A 仅存储) */
  triggers?: string[];
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
  kind: 'docx' | 'xlsx' | 'pdf' | 'pptx' | 'epub' | 'html' | 'text' | 'image' | 'other';
  size: number;
  /** 文本类附件的提取文本(已截断) */
  extracted_text: string | null;
  /** 文档指纹(路径 sha256 前 12 位),doc_search / doc_read 定位用(R3 C1) */
  doc_id?: string | null;
  /** 已驻留的块数(缺省 = 未驻留,回退全文注入) */
  chunk_count?: number | null;
  /** 200 字摘要(「文档卡」注入用) */
  summary?: string | null;
  /** 提取失败/无文本的原因(F7 失败显性化) */
  extract_note?: string | null;
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
      type: 'tool_call_result';
      tool_call_id: string;
      tool_name: string;
      server_name: string;
      content: string;
      ok: boolean;
    }
  | {
      type: 'done';
      finish_reason: string | null;
      prompt_tokens: number | null;
      completion_tokens: number | null;
      /** 思考 token(仅计数无明文思考的端点如 OpenAI 官方) */
      reasoning_tokens: number | null;
    }
  | { type: 'error'; message: string };
