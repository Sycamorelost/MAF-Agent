import { invoke } from '@tauri-apps/api/core';

// ============ websearch(通用联网搜索;P0 自 research 迁出,IPC 名不变)============

export interface SourceItem {
  id: number;
  title: string;
  url: string;
  snippet: string;
  engine: string;
  published: string | null;
}

/** 调用方式:api = 内建直连引擎;mcp = 走 MCP 服务器(预置「联网搜索」= Tavily 官方 MCP)。 */
export type SearchMode = 'api' | 'mcp';

export interface SearchConfigDto {
  mode: SearchMode;
  engine: string;
  base_url: string;
  has_api_key: boolean;
  mcp_server: string;
  mcp_tool: string;
  /** 当前 MCP server 是否需要 Key(零 Key server 如 agent-search-mcp = false) */
  mcp_needs_key: boolean;
  /** MCP server 是否已就绪(已启动,或配置中存在可懒启动) */
  mcp_ready: boolean;
  /** server 显示名(含 args,如 "联网搜索（npx -y agent-search-mcp@latest）") */
  mcp_server_display: string;
  /** 已启动 server 实时探测到的工具名;未启动时为空 */
  mcp_tool_names: string[];
}

export const getSearchConfig = () => invoke<SearchConfigDto>('get_search_config');

export const saveSearchConfig = (input: {
  mode?: SearchMode;
  engine: string;
  base_url: string;
  api_key?: string | null;
  mcp_server?: string;
  mcp_tool?: string;
}) => invoke<void>('save_search_config', { input });

/** 测试用草稿:设置页未保存时按当前选择验证切换是否生效(不落盘)。 */
export interface SearchTestDraft {
  mode?: SearchMode;
  engine?: string;
  base_url?: string;
  mcp_server?: string;
  mcp_tool?: string;
}

export const searchTest = (query: string, draft?: SearchTestDraft) =>
  invoke<SourceItem[]>('search_test', { query, draft: draft ?? null });
