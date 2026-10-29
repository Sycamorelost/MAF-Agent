/** 去 AI 味 · 提示词与文案组装(纯函数,无 React 依赖)。 */

import * as ipc from '../../lib/ipc';
import { REWRITE_LEVELS } from './utils';

/** 改写师的用户消息:档位 + 检测模板特征 + 风格锚点 + 原文。 */
export function buildRewriteUserMsg(
  text: string,
  rewriteLevel: string,
  report: ipc.AiflavorReport,
  anchor: string,
): string {
  const lvl = REWRITE_LEVELS.find((l) => l.id === rewriteLevel);
  const topRules = report.rule_hits
    .slice(0, 6)
    .map((r) => `${r.name}×${r.count}`)
    .join('、');
  return [
    '【任务】对下文做人味化改写。',
    `【改写档位】${lvl?.label ?? rewriteLevel}(${lvl?.desc ?? ''})`,
    topRules ? `【检测发现的模板特征】${topRules}` : '',
    anchor.trim() ? `【风格锚点(用户旧作特征,深度档以此为准)】\n${anchor.trim()}` : '',
    `【原文】\n${text}`,
  ]
    .filter(Boolean)
    .join('\n\n');
}

/** 检测官 / 学术合规 / 档案员的输入组装;返回 null = 前置条件不足(已提示)。
 *  纯函数化后:null 表示未知智能体,{ msg } 表示前置条件不足,{ userMsg } 表示可发送。 */
export function buildAgentMsg(p: {
  agentId: string;
  report: ipc.AiflavorReport | null;
  text: string;
  anchor: string;
  discModel: string;
  discUse: string;
}): { userMsg: string } | { msg: string } | null {
  const { agentId, report, text, anchor, discModel, discUse } = p;
  if (agentId === 'ai-flavor-detector') {
    if (!report) {
      return { msg: '请先点击「开始检测」生成本地统计结果，检测官只解读已有证据' };
    }
    const origin = text.trim() || report.sentence_marks.map((s) => s.text).join('');
    const dims = report.dims.map((d) => `${d.name} ${d.score}(${d.level}；原始值 ${d.raw})`).join('；');
    const rules = report.rule_hits.map((r) => `${r.name}×${r.count}`).join('、') || '无';
    const hot = report.sentence_marks
      .filter((s) => s.heat > 0)
      .slice(0, 20)
      .map((s) => `- [${s.heat >= 3 ? '高度' : s.heat === 2 ? '中度' : '轻度'}] ${s.text}${s.reasons.length ? `（命中:${s.reasons.join(';')}）` : ''}`)
      .join('\n');
    return {
      userMsg: [
        '【任务】解读下列本地统计结果，按你的输出格式给出证据化研判。',
        `【总分】${report.score}（${report.level}）`,
        `【六维】${dims}`,
        `【命中规则】${rules}`,
        hot ? `【高热度句子】\n${hot}` : '【高热度句子】无',
        origin ? `【原文】\n${origin}` : '',
      ]
        .filter(Boolean)
        .join('\n\n'),
    };
  }
  if (agentId === 'academic-voice') {
    if (!text.trim()) {
      return { msg: '请先在左侧输入要润色的文本' };
    }
    return {
      userMsg: [
        '【任务】对下文做学术语体润色，并生成 AI 使用披露声明。',
        discModel.trim() ? `【模型名】${discModel.trim()}` : '',
        `【使用用途】${discUse.trim() || '辅助语言润色'}`,
        `【原文】\n${text}`,
      ]
        .filter(Boolean)
        .join('\n\n'),
    };
  }
  if (agentId === 'style-anchor') {
    const sample = anchor.trim() || text.trim();
    if (!sample) {
      return { msg: '请先在「风格锚点」里粘贴旧文，或在输入区放入样本' };
    }
    return {
      userMsg: ['【任务】从下列样本提炼风格锚点卡，作为改写时的人格化依据。', `【样本】\n${sample}`].join('\n\n'),
    };
  }
  return null;
}

/** 学术披露声明草稿(纯前端模板,C.3-5)。 */
export function buildDisclosure(discModel: string, discUse: string): string {
  return [
    'AI 使用披露声明(草稿)',
    `在本文的撰写过程中,作者使用了「${discModel || '(模型名)'}」辅助完成${discUse || '(用途)'}。`,
    `生成日期:${new Date().toLocaleDateString('zh-CN')};所有由该工具生成或改写的内容均经过作者逐一审阅、核实与修改,作者对本文的全部内容承担责任。`,
    '依据 Nature/Springer 政策,该使用情况已在 Methods(或致谢)中如实披露;生成式 AI 不作为本文作者署名。',
  ].join('\n');
}
