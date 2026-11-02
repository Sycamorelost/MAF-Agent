/**
 * 去 AI 味（「人味」）提示词契约。
 *
 * 设计依据: dev_plan/去AI味提示词调研与落地方案_2026-11-02.md
 * 原则:
 * - 只给「不要什么」,不给固定句式模板,避免所有回答变成同一种腔调;
 * - 事实、数据、引用、术语、代码零改动,优先级高于文风;
 * - 白名单注入:只有明确列出的生成型智能体才追加;代码/翻译/JSON/合规划为 off。
 */

export type HumanVoiceLevel = 'off' | 'light' | 'standard';

/** 契约标题标记:核心提示已包含时不再重复追加。 */
export const HUMAN_VOICE_MARK = '## 文字质感';

/** 标准档:日常问答、写作、邮件、周报、讲解。 */
export const HUMAN_VOICE_STANDARD = `${HUMAN_VOICE_MARK}
在不改变事实、数据、引用和结论的前提下，让文字读起来像人写的：
- 先给结论或事实，再给必要解释；不靠「值得注意的是」「由此可见」「综上所述」这类连接词撑段落。
- 句长要有变化，避免每段都是「观点句 + 解释句 + 总结句」的三段式，也不连续排比。
- 不用「不是 X，而是 Y」的否定对偶开场；不给普通事实加「标志着 / 彰显了 / 开启新篇章」的意义拔高。
- 不虚构来源与权威：没有出处就不写「有专家认为 / 业内普遍」；不确定就直说不确定。
- 不写客服腔（「希望对你有帮助」「如有其他问题随时告诉我」）与知识截止免责模板。
- 少用破折号、粗体、emoji 做装饰；标题只在真正分节时使用。
- 保留用户原有用词与语域；专业术语、代码、数字、引文一律不动。
- 不要为了「像人」编造个人经历、情绪或第一人称故事。`;

/** 轻档:研究报告、文档解读、论文阅读 —— 只去套话与拔高,不改结构。 */
export const HUMAN_VOICE_LIGHT = `${HUMAN_VOICE_MARK}
在不改变事实、数据、引用和结论的前提下：
- 不靠「值得注意的是」「由此可见」「综上所述」这类连接词撑段落。
- 不给普通事实加「标志着 / 彰显了 / 开启新篇章」的意义拔高。
- 不虚构来源与权威：没有出处就不写「有专家认为 / 业内普遍」。`;

/**
 * 生成型智能体档位表(白名单)。
 * 未列出的智能体一律不注入:代码/翻译/JSON/合规/用户自定义人格等场景,
 * 「人味」会破坏格式、保真度或严谨性。
 */
const AGENT_LEVEL: Record<string, Exclude<HumanVoiceLevel, 'off'>> = {
  // 标准档:用户感知最强的日常生成
  'product-assistant': 'standard',
  'chinese-assistant': 'standard',
  'meeting-minutes': 'standard',
  'report-writer': 'standard',
  'weekly-report': 'standard',
  'email-drafter': 'standard',
  'feynman-coach': 'standard',
  // 轻档:严谨叙述,只去套话与拔高
  'deep-researcher': 'light',
  'doc-analyst': 'light',
  'paper-reader': 'light',
};

/** 解析设置里的档位字符串;未知/空值回落标准档。 */
export function parseHumanVoiceLevel(raw: string | null | undefined): HumanVoiceLevel {
  return raw === 'off' || raw === 'light' || raw === 'standard' ? raw : 'standard';
}

/** 契约文本;off 返回空串(不注入)。 */
export function humanVoiceText(level: HumanVoiceLevel): string {
  if (level === 'light') return HUMAN_VOICE_LIGHT;
  if (level === 'standard') return HUMAN_VOICE_STANDARD;
  return '';
}

/**
 * 计算某次请求实际使用的档位。
 * - 全局 off:一律关闭;
 * - 无智能体(默认助手):跟随全局档位;
 * - 有智能体:未在白名单内 = 关闭;全局 light 时把标准档降为 light。
 */
export function resolveHumanVoice(
  globalLevel: HumanVoiceLevel,
  agentId?: string | null,
): HumanVoiceLevel {
  if (globalLevel === 'off') return 'off';
  if (!agentId) return globalLevel;
  const agentLevel = AGENT_LEVEL[agentId];
  if (!agentLevel) return 'off';
  return globalLevel === 'light' ? 'light' : agentLevel;
}
