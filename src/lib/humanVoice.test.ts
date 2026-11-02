import { describe, it, expect } from 'vitest';
import {
  HUMAN_VOICE_MARK,
  HUMAN_VOICE_LIGHT,
  HUMAN_VOICE_STANDARD,
  humanVoiceText,
  parseHumanVoiceLevel,
  resolveHumanVoice,
} from './humanVoice';

describe('parseHumanVoiceLevel', () => {
  it('合法档位原样返回', () => {
    expect(parseHumanVoiceLevel('off')).toBe('off');
    expect(parseHumanVoiceLevel('light')).toBe('light');
    expect(parseHumanVoiceLevel('standard')).toBe('standard');
  });

  it('未知/空值回落标准档', () => {
    expect(parseHumanVoiceLevel(null)).toBe('standard');
    expect(parseHumanVoiceLevel(undefined)).toBe('standard');
    expect(parseHumanVoiceLevel('')).toBe('standard');
    expect(parseHumanVoiceLevel('STANDARD')).toBe('standard');
    expect(parseHumanVoiceLevel('hype')).toBe('standard');
  });
});

describe('humanVoiceText', () => {
  it('off 不注入(空串)', () => {
    expect(humanVoiceText('off')).toBe('');
  });

  it('light / standard 均带统一标记', () => {
    expect(humanVoiceText('light')).toBe(HUMAN_VOICE_LIGHT);
    expect(humanVoiceText('standard')).toBe(HUMAN_VOICE_STANDARD);
    expect(HUMAN_VOICE_LIGHT.startsWith(HUMAN_VOICE_MARK)).toBe(true);
    expect(HUMAN_VOICE_STANDARD.startsWith(HUMAN_VOICE_MARK)).toBe(true);
  });

  it('契约只约束文风,不引入固定句式模板', () => {
    expect(HUMAN_VOICE_STANDARD).not.toContain('请使用以下模板');
    expect(HUMAN_VOICE_STANDARD).toContain('不改变事实');
    expect(HUMAN_VOICE_STANDARD).toContain('保留用户原有用词');
  });
});

describe('resolveHumanVoice(白名单与降级)', () => {
  it('全局关闭时一律关闭(包括白名单智能体)', () => {
    expect(resolveHumanVoice('off', 'product-assistant')).toBe('off');
    expect(resolveHumanVoice('off', null)).toBe('off');
  });

  it('无智能体(默认助手)跟随全局档位', () => {
    expect(resolveHumanVoice('standard', null)).toBe('standard');
    expect(resolveHumanVoice('light', null)).toBe('light');
  });

  it('白名单内标准档智能体注入标准档', () => {
    expect(resolveHumanVoice('standard', 'product-assistant')).toBe('standard');
    expect(resolveHumanVoice('standard', 'weekly-report')).toBe('standard');
  });

  it('白名单外智能体不注入(代码/翻译/JSON 等语义保真场景)', () => {
    expect(resolveHumanVoice('standard', 'code-assistant')).toBe('off');
    expect(resolveHumanVoice('standard', 'translator')).toBe('off');
    expect(resolveHumanVoice('standard', 'custom-user-agent')).toBe('off');
  });

  it('全局 light 时白名单智能体统一降为轻档', () => {
    expect(resolveHumanVoice('light', 'product-assistant')).toBe('light');
    expect(resolveHumanVoice('light', 'deep-researcher')).toBe('light');
  });

  it('轻档智能体在标准档下保持轻档', () => {
    expect(resolveHumanVoice('standard', 'paper-reader')).toBe('light');
  });
});
