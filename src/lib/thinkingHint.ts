import type { ThinkingLevel } from '../stores/thinking';

/** 档位显示名。 */
export const THINKING_LEVEL_LABEL: Record<ThinkingLevel, string> = {
  light: 'Light',
  medium: 'Medium',
  high: 'High',
  ultra: 'Ultra',
};

/**
 * 档位 → 该模型实际发送的参数值(透明化展示用)。
 * ⚠️ 与后端 openai.rs::apply_reasoning_params 五家映射表保持一致,两侧改动必须同步。
 */
export function effortHint(providerId: string, model: string, level: ThinkingLevel): string {
  const m = model.toLowerCase();
  const lowHighMax = (l: ThinkingLevel) => (l === 'light' ? 'low' : l === 'ultra' ? 'max' : 'high');
  switch (providerId) {
    case 'openai':
      return level === 'light' ? 'low' : level === 'medium' ? 'medium' : 'high';
    case 'deepseek':
      return `thinking + ${lowHighMax(level)}`;
    case 'kimi':
      if (m.includes('k3') || m.includes('k2.7')) return `effort ${lowHighMax(level)}`;
      return 'thinking on'; // K2.6 仅开关,档位控件不可用
    case 'qwen': {
      const budget = level === 'light' ? '2048' : level === 'medium' ? '8192' : level === 'high' ? '24576' : '不限';
      return `budget ${budget}`;
    }
    case 'zhipu':
      if (m.includes('glm-5.3')) return `effort ${lowHighMax(level)}`;
      if (m.includes('glm-5.2'))
        return `effort ${level === 'light' ? 'low' : level === 'medium' ? 'medium' : level === 'high' ? 'high' : 'max'}`;
      return 'thinking on'; // GLM-4.5/4.6 仅开关
    default:
      return `reasoning_effort ${level === 'light' ? 'low' : level === 'medium' ? 'medium' : 'high'}`;
  }
}
