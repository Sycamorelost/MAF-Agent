import { describe, it, expect } from 'vitest';
import {
  withBase,
  withProtocol,
  DEFAULT_SYSTEM_PROMPT,
  BASE_IDENTITY,
  BASE_PHYSICAL_CONTRACT,
  CONTRACT_MARK,
} from './systemPrompt';

describe('withBase(R3 分层)', () => {
  it('空核心提示回退:默认人格 + 身份锚 + 物理契约', () => {
    const out = withBase('');
    expect(out).toBe(
      `${DEFAULT_SYSTEM_PROMPT}\n\n${BASE_IDENTITY}\n\n${BASE_PHYSICAL_CONTRACT}`,
    );
  });

  it('纯空白核心提示同样回退默认', () => {
    expect(withBase('   \n ')).toBe(
      `${DEFAULT_SYSTEM_PROMPT}\n\n${BASE_IDENTITY}\n\n${BASE_PHYSICAL_CONTRACT}`,
    );
  });

  it('自定义核心提示(智能体人格)追加身份锚与物理契约', () => {
    const out = withBase('你是翻译助手');
    expect(out.startsWith('你是翻译助手')).toBe(true);
    expect(out).toContain(BASE_IDENTITY);
    expect(out).toContain(BASE_PHYSICAL_CONTRACT);
  });

  it('identity=false(全裸模式)不追加身份锚,保留物理契约', () => {
    const out = withBase('你是翻译助手', { identity: false });
    expect(out.startsWith('你是翻译助手')).toBe(true);
    expect(out).not.toContain(BASE_IDENTITY);
    expect(out).toContain(BASE_PHYSICAL_CONTRACT);
  });

  it('全裸模式 + 空核心:默认人格仍兜底(人格不可缺位)', () => {
    const out = withBase('', { identity: false });
    expect(out).toBe(`${DEFAULT_SYSTEM_PROMPT}\n\n${BASE_PHYSICAL_CONTRACT}`);
  });

  it('核心提示首尾空白被裁剪', () => {
    const out = withBase('  你是翻译助手  ');
    expect(out.startsWith('你是翻译助手')).toBe(true);
  });

  it('已包含物理契约标记时不重复追加(原样返回)', () => {
    const core = `自定义前缀\n\n${BASE_PHYSICAL_CONTRACT}`;
    expect(withBase(core)).toBe(core);
  });

  it('物理契约只保留物理事实:不含人格化身份句', () => {
    expect(BASE_PHYSICAL_CONTRACT).toContain(CONTRACT_MARK);
    expect(BASE_PHYSICAL_CONTRACT).toContain('连续失败 3 次');
    expect(BASE_PHYSICAL_CONTRACT).not.toContain('你是');
  });

  it('身份锚只声明运行环境与不冒充,不抢智能体角色(R3-D 合并)', () => {
    expect(BASE_IDENTITY).not.toMatch(/你是.*助手/);
    expect(BASE_IDENTITY).toContain('以当前智能体设定为准');
  });

  it('Base 层规模受控:契约 ≤ 1000 字符(R3-D 加审批段后,原 900 已触顶)', () => {
    expect(BASE_PHYSICAL_CONTRACT.length).toBeLessThanOrEqual(1000);
  });
});

describe('withProtocol(兼容别名)', () => {
  it('等价于 withBase(core)', () => {
    expect(withProtocol('你是翻译助手')).toBe(withBase('你是翻译助手'));
  });
});
