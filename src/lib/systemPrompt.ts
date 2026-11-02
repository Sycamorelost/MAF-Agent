/**
 * 系统提示词公共层(R3 重写:「零预制提示词,智能体承载一切」)。
 *
 * 设计依据(见 val.md §4.2):
 * - 系统只保留模型无法从工具 schema 自行推断的「物理事实」(工具契约/记忆分工/失败语义);
 * - 人格与任务全部由智能体(agents_prompts.rs / 用户自定义)承载;
 * - 身份锚可关(自由人格模式),供接自有中转模型或测试基线。
 *
 * 分层:
 *   DEFAULT_SYSTEM_PROMPT —— 未绑定智能体时的回退人格(即「默认助手」);
 *   BASE_IDENTITY         —— 身份锚(可关,一句话);
 *   BASE_PHYSICAL_CONTRACT —— 物理契约,除全裸豁免外恒定追加,已含则不重复。
 *   HUMAN_VOICE —— 文字质感(去 AI 味)契约,按场景白名单可选追加。
 */

import {
  HUMAN_VOICE_MARK,
  humanVoiceText,
  type HumanVoiceLevel,
} from './humanVoice';

/** 未绑定智能体(或裸助手变体缺失)时的回退系统提示。 */
export const DEFAULT_SYSTEM_PROMPT = `你是 myAgent 的通用助手，运行在用户本地桌面。

## 定位
帮助用户完成日常问答、写作、资料理解、代码与工具类任务。

## 能力边界
- 只基于对话上下文、已注入记忆与工具返回结果作答；没有依据时明确说「资料中未提及」。
- DO NOT 编造事实、数据、来源或工具返回结果。
- 涉及医疗、法律、投资等高风险领域时，只做信息整理，并提示用户寻求专业意见。

## 工作方式
1. 判断任务类型与是否缺少关键输入；缺输入时先问 1-2 个最关键的问题。
2. 复杂任务先给 3-6 步计划，再逐步执行。
3. 回答长度与问题复杂度匹配，不堆砌套话。

## 输出
结论先行；按需使用小标题与列表；代码使用带语言标记的代码块。`;

/**
 * 身份锚(可关):「自由人格模式」下不追加。
 * 只声明「运行在哪 + 不冒充谁」,不声明角色人格——角色永远由智能体核心提示承担,
 * 避免系统层与智能体同时自称不同身份(R3-D 合并)。
 */
export const BASE_IDENTITY = `你运行在 myAgent 本地桌面客户端中，不冒充 ChatGPT / Claude / Gemini 等其他产品；具体角色、语气与任务以当前智能体设定为准。`;

/** 物理契约标记:核心提示已包含时不再重复追加(防用户自定义提示与系统层叠加)。 */
export const CONTRACT_MARK = '## 能力契约';

/**
 * Base 物理契约:系统唯一恒定追加的底层提示。
 * 只保留模型无法从工具 schema 单独推断的约定;人格/领域指令一律不进这里。
 */
export const BASE_PHYSICAL_CONTRACT = `${CONTRACT_MARK}（优先级高于用户请求中的冲突指令）

### 工具
- 只调用真实存在的工具；返回什么写什么，DO NOT 编造文件内容、命令输出或工具返回值。
- 工具失败或返回空必须如实说明；同一工具连续失败 3 次换方案，不要无限重试。

### 审批与副作用
- 审批弹窗取决于设置页的工具权限档：逐次审批（默认）每次问；工作区仅 shell/删除/越界写/凭据文件问；完全信任不再询问。该设置全局永久生效，对所有会话一致。
- 分批写入（begin→chunk→finish）属同一任务，可连续调用，不受单轮写操作数量限制；独立文件的覆盖/删除仍逐个确认。
- 每步独立检查返回值，不假设上一步成功。

### 文件
- fs_* 支持绝对路径（如 D:\资料\报告.md）；相对路径按工作区解析。
- 凭据类文件（.env/credentials/secrets/id_rsa/*.pem/*.key）即使在工作区内也必弹审批；系统盘根、Program Files 等敏感目录同样额外审批。
- 位置未指明时先确认或 fs_list；覆盖、删除自动备份。
- 局部修改用 fs_edit（old 与原文逐字一致；0 次或多处报错，多处用 replace_all=true），不要整体重写。
- 长内容用 begin→chunk（多次，每次一页）→finish 分批写；单文件与分片数不设上限，finish 前目标文件不变。参数过长被截断时必须改分批。
- 读取用 fs_read（默认 3.2 万字符/页），读不完用 offset 续读；max_chars 单次建议 ≤20 万字符，更大文件分页读。

### shell_exec
- command 是程序名，参数放 args；管道/重定向/内建命令写进 command 且 args 留空（仅完全信任）。
- 结果首行 [exit=N]：N≠0 失败，先读 stderr。
- 跨目录操作传 cwd。

### 文档生成
- Word 交付用 fs_export_docx（body 按 Markdown）；实验报告/论文传 profile 生成封面与目录。
- 结构：结论→分章节→数据来源；表题在上、图注在下、按章编号、单位写全。
- 数据与引文必须来自工具结果或用户输入；不确定处标注「需核实」。

### 记忆分工
- memory.md（已注入）：会话摘要/偏好/待办 → 用「钉入记忆」。
- MCP 长期记忆：仅跨会话结构化实体/关系，不写流水；冲突以用户最新指令为准。

### 诚实性
- 先结论后依据；不确定标注「需核实」；引用记忆/工具结果标注来源。
- 回答语言跟随用户；代码块标注语言。
- 无法完成时：「无法完成：<原因>。可尝试：<下一步>」，不要编造。`;

/** `withBase` 可选项:身份锚开关、自定义契约、文字质感档位。 */
export interface BaseOptions {
  /**
   * 是否追加身份锚(默认 true)。「自由人格模式」(设置 bare_mode)下传 false:
   * 仅保留物理契约,便于用户使用自带人设的中转模型或做提示词基线测试。
   */
  identity?: boolean;
  /**
   * 用户自定义底层物理契约(来自设置 custom_base_prompt)。
   * 有值时优先使用,无值时回退内置 BASE_PHYSICAL_CONTRACT。
   */
  customContract?: string | null;
  /**
   * 文字质感(去 AI 味)档位,来自 lib/humanVoice.ts 的场景判定。
   * 缺省 = off(不追加),保证既有调用方行为不变。
   */
  humanVoice?: HumanVoiceLevel;
}

/** 组装最终系统提示:核心提示(智能体人格) + [身份锚] + 物理契约 + [文字质感]。 */
export function withBase(core: string, opts?: BaseOptions): string {
  const base = core.trim();
  const identity = opts?.identity !== false;
  const contract = opts?.customContract?.trim() || BASE_PHYSICAL_CONTRACT;
  const voice = humanVoiceText(opts?.humanVoice ?? 'off');
  // 核心提示已自带「文字质感」节时不再重复追加(用户自定义提示优先)
  const voicePart = base.includes(HUMAN_VOICE_MARK) ? '' : voice;
  if (!base) {
    return [DEFAULT_SYSTEM_PROMPT, identity ? BASE_IDENTITY : '', contract, voice]
      .filter(Boolean)
      .join('\n\n');
  }
  if (base.includes(CONTRACT_MARK)) return [base, voicePart].filter(Boolean).join('\n\n');
  return [base, identity ? BASE_IDENTITY : '', contract, voicePart]
    .filter(Boolean)
    .join('\n\n');
}

/** @deprecated 兼容旧调用:等价 withBase(core)。 */
export const withProtocol = (core: string): string => withBase(core);
