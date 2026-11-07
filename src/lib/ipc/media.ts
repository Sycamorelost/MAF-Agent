import { invoke } from '@tauri-apps/api/core';

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

/** 历史作品条目:缩略图 + meta。 */
export interface ImageListItem {
  path: string;
  /** 256px JPEG 缩略图 data URL */
  thumb: string;
  meta: ImageMeta | null;
}

export const generateImage = (input: GenerateImageInput) =>
  invoke<ImageResultItem[]>('generate_image', { input });

/** 历史作品列表(含缩略图)。 */
export const listImages = () => invoke<ImageListItem[]>('list_images');

export const readImage = (path: string) => invoke<string>('read_image', { path });

/** 「下载」:原生另存为对话框 + 复制到选定路径;返回保存路径(取消为 null)。 */
export const saveImageAs = (srcPath: string) =>
  invoke<string | null>('save_image_as', { srcPath });
