import { invoke } from '@tauri-apps/api/core';


// ============ 全局搜索 ============

export interface SessionHit {
  id: string;
  title: string;
  updated: string;
}

export interface MessageHit {
  session_id: string;
  session_title: string;
  message_id: number;
  role: string;
  snippet: string;
  created: string;
  /** 消息所属分支(F4;跳转时若与会话激活分支不同,先切换再定位) */
  branch_id: string;
}

export interface MemoryHit {
  session_id: string;
  session_title: string;
  snippet: string;
}

/** 知识库命中(迭代 D8)。 */
export interface NoteHit {
  id: string;
  title: string;
  snippet: string;
  updated: string;
}

export interface SearchResults {
  sessions: SessionHit[];
  messages: MessageHit[];
  memory: MemoryHit[];
  notes: NoteHit[];
}

export const searchAll = (query: string) => invoke<SearchResults>('search_all', { query });

// ============ 知识库 / 笔记库(迭代 D8) ============

export interface NoteMeta {
  id: string;
  title: string;
  tags: string[];
  created: string;
  updated: string;
  /** 正文摘要(列表预览) */
  excerpt: string;
}

export interface NoteDoc {
  id: string;
  title: string;
  tags: string[];
  created: string;
  updated: string;
  body: string;
}

export const notesList = () => invoke<NoteMeta[]>('notes_list');

export const notesGet = (id: string) => invoke<NoteDoc>('notes_get', { id });

/** 新建/保存笔记(id=null 表示新建)。 */
export const notesSave = (id: string | null, title: string, tags: string[], body: string) =>
  invoke<NoteDoc>('notes_save', { id, title, tags, body });

export const notesDelete = (id: string) => invoke<void>('notes_delete', { id });


