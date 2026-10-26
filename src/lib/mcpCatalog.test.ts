import { describe, it, expect } from 'vitest';
import { toolDesc, serverDesc, SERVER_DESC } from './mcpCatalog';

describe('toolDesc', () => {
  it('已知工具返回中文说明', () => {
    expect(toolDesc('read_text_file')).toBe('读取一个文本文件的完整内容');
    expect(toolDesc('create_entities')).toBe('新建实体节点(人物/项目/概念等)');
  });

  it('未知工具回退到服务器原文', () => {
    expect(toolDesc('unknown_tool', 'server description')).toBe('server description');
  });

  it('未知工具且无 fallback 时返回空串', () => {
    expect(toolDesc('unknown_tool')).toBe('');
  });
});

describe('serverDesc', () => {
  it('预置 server 返回中文说明', () => {
    expect(serverDesc('项目文件')).toBe(SERVER_DESC['项目文件']);
    expect(serverDesc('长期记忆')).toContain('知识图谱');
  });

  it('自定义 server 返回空串', () => {
    expect(serverDesc('my-custom-server')).toBe('');
  });
});
