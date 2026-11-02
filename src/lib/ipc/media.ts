﻿import { invoke, Channel } from '@tauri-apps/api/core';
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
  /** 实际发给模型的组装后 Prompt(旧作品无此字段) */
  final_prompt?: string | null;
  /** 模型族: sdxl / flux / openai / cogview / generic */
  prompt_family?: string | null;
  /** 组装后 Prompt 的估算 token 数 */
  est_tokens?: number | null;
}

/** 前端实时预览用的 Prompt 组装输入(与 generate_image 同一套规则)。 */
export interface ComposeDrawPromptInput {
  provider_id: string;
  model: string;
  prompt: string;
  negative?: string | null;
  style_prompt?: string | null;
}

/** 组装结果:预览看到的内容就是实际发给模型的内容。 */
export interface ComposedPrompt {
  prompt: string;
  family: string;
  family_label: string;
  max_chars: number;
  chars: number;
  est_tokens: number;
  truncated: boolean;
  warnings: string[];
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

/** 只组装 Prompt(不发起生成),用于绘图页实时显示字数 / token / 截断告警。 */
export const composeDrawPrompt = (input: ComposeDrawPromptInput) =>
  invoke<ComposedPrompt>('compose_draw_prompt', { input });

export const listImages = () => invoke<ImageListItem[]>('list_images');

export const readImage = (path: string) => invoke<string>('read_image', { path });

/** 「下载」:原生另存为对话框 + 复制到选定路径;返回保存路径(取消为 null)。 */
export const saveImageAs = (srcPath: string) =>
  invoke<string | null>('save_image_as', { srcPath });

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

/** 预置术语导入报告(内置 8 类专业术语表双向展开,合并不覆盖) */
export interface GlossaryImportReport {
  /** 本次新增词条数 */
  added: number;
  /** 与现有词条重复、保留用户译法的条数 */
  kept: number;
  /** 合并后术语表总条数 */
  total: number;
  /** 预置来源清单(分类:权威来源) */
  sources: string[];
}

export const glossaryImportPresets = () =>
  invoke<GlossaryImportReport>('glossary_import_presets');

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



