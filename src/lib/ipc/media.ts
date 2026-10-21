import { invoke, Channel } from '@tauri-apps/api/core';
import { cancelStream } from './provider';

import type { AttachmentMeta, StreamChunk } from './types';

// ============ AI 绘画 ============

export interface GenerateImageInput {
  provider_id: string;
  model: string;
  prompt: string;
  /** 负面提示(后端合并进 prompt) */
  negative?: string | null;
  /** 风格英文关键词(后端合并进 prompt) */
  style_prompt?: string | null;
  /** 风格标签名(仅 meta 留档) */
  style?: string | null;
  size?: string | null;
  n?: number | null;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
}

export interface ImageResultItem {
  /** 落盘绝对路径(data/images/ 下) */
  path: string;
  /** base64 data URI,直接渲染 */
  data_url: string;
  revised_prompt: string | null;
}

/** 生成参数留档(与 png 同名同目录的 meta.json)。 */
export interface ImageMeta {
  prompt: string;
  negative: string | null;
  style: string | null;
  size: string;
  model: string;
  provider_id: string;
  seed: number | null;
  guidance: number | null;
  created: string;
}

/** 历史作品条目:缩略图 + meta。 */
export interface ImageListItem {
  path: string;
  /** 256px JPEG 缩略图 data URL */
  thumb: string;
  meta: ImageMeta | null;
}

export const generateImage = (input: GenerateImageInput) =>
  invoke<ImageResultItem[]>('generate_image', { input });

export const listImages = () => invoke<ImageListItem[]>('list_images');

export const readImage = (path: string) => invoke<string>('read_image', { path });

export const imageToSession = (imagePath: string, sessionId: string) =>
  invoke<AttachmentMeta>('image_to_session', { imagePath, sessionId });

// ============ AI 翻译 ============

export interface TranslateStreamInput {
  provider_id: string;
  model: string;
  text: string;
  /** 源语言英文名;空/null = 自动检测 */
  source_lang?: string | null;
  /** 目标语言英文名 */
  target_lang: string;
  custom_base_url?: string | null;
  custom_display_name?: string | null;
  /** 前端生成的取消句柄;cancel() 时通知后端中止请求(M-2) */
  cancel_id?: string;
}

// ---- 术语表(迭代 D6.1)----

export interface GlossaryEntry {
  src: string;
  dst: string;
  note?: string | null;
}

export const glossaryList = () => invoke<GlossaryEntry[]>('glossary_list');

export const glossarySave = (entries: GlossaryEntry[]) =>
  invoke<void>('glossary_save', { entries });

// ---- 翻译历史(迭代 D6.2)----

export interface TranslationRecord {
  /** 落盘文件名(list 返回时携带;保存时忽略) */
  file_name?: string;
  created: string;
  provider_id: string;
  model: string;
  source_lang: string | null;
  target_lang: string;
  source_text: string;
  result_text: string;
}

export const saveTranslation = (rec: TranslationRecord) =>
  invoke<string>('save_translation', { rec });

export const listTranslations = () => invoke<TranslationRecord[]>('list_translations');

export const deleteTranslation = (fileName: string) =>
  invoke<void>('delete_translation', { fileName });

/** 流式翻译。用法同 chatStream:onChunk 收增量,done 完成时 resolve。 */
export function translateStream(
  input: TranslateStreamInput,
  onChunk: (chunk: StreamChunk) => void,
): { done: Promise<void>; cancel: () => void } {
  const channel = new Channel<StreamChunk>();
  let cancelled = false;
  let resolveDone!: () => void;
  let rejectDone!: (e: unknown) => void;
  const done = new Promise<void>((res, rej) => {
    resolveDone = res;
    rejectDone = rej;
  });

  channel.onmessage = (chunk) => {
    if (cancelled) return;
    onChunk(chunk);
    if (chunk.type === 'done') {
      resolveDone();
    } else if (chunk.type === 'error') {
      rejectDone(new Error(chunk.message));
    }
  };

  const cancelId = input.cancel_id ?? crypto.randomUUID();
  void invoke<void>('translate_stream', {
    input: { ...input, cancel_id: cancelId },
    channel,
  }).catch((e) => {
    if (!cancelled) rejectDone(e);
  });

  return {
    done,
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      cancelStream(cancelId);
      resolveDone();
    },
  };
}



