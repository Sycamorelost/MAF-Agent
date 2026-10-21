import { invoke } from '@tauri-apps/api/core';


// ============ 教程卡(迭代 D7.2) ============

/** 一张教程卡:tutorials/*.md 的 front-matter + markdown 正文。 */
export interface TutorialDoc {
  id: string;
  /** 所属页面:chat / research / draw / translate */
  page: string;
  title: string;
  summary: string;
  order: number;
  content: string;
}

export const listTutorials = (page: string) => invoke<TutorialDoc[]>('list_tutorials', { page });

