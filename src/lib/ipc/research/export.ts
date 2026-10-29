import { invoke } from '@tauri-apps/api/core';

// ============ 科研增强 + 报告导出(v1.9.0 工作块 II) ============

/** 内置科研方法论技能文档(健康检查用)。 */
export interface SkillDoc {
  /** 相对 resources/skills 的路径 */
  file: string;
  bytes: number;
  ok: boolean;
}

export const researchListSkills = () => invoke<SkillDoc[]>('research_list_skills');

/** 后端 H-2:目标已存在但未确认覆盖时返回的错误码。 */
export const EXPORT_TARGET_EXISTS = 'TARGET_EXISTS';

/** 导出报告 Markdown(后端同目录复制 figures/ 资源夹),返回实际写入路径。
 *  token 为 pickSavePath 签发的路径令牌(N-6:后端只认令牌,不认裸路径);
 *  overwrite=false 且目标已存在时 reject(EXPORT_TARGET_EXISTS),确认后带 true 重试。 */
export const researchExportMarkdown = (id: string, token: string, overwrite: boolean) =>
  invoke<string>('research_export_markdown', { id, token, overwrite });

/** 导出自包含 HTML(前端传已渲染 DOM,后端内嵌 figures 为 base64),返回实际写入路径。 */
export const researchExportHtml = (
  id: string,
  html: string,
  token: string,
  overwrite: boolean,
) => invoke<string>('research_export_html', { id, html, token, overwrite });

/** 导出报告为 Word 文档(docx):后端复用 docgen 学术版式,与「文档生成」同一套模板。
 *  profile 留空则按标题/正文自动识别用途(实验/论文/实践/通用)。 */
export const researchExportDocx = (
  id: string,
  token: string,
  overwrite: boolean,
  profile?: string,
) => invoke<string>('research_export_docx', { id, token, overwrite, profile: profile ?? null });

/** 读取研究图示为 data URL(报告内联预览;仅 figures 目录内图片)。 */
export const researchFigureData = (id: string, name: string) =>
  invoke<string>('research_figure_data', { id, name });

/** 参考资料元数据(不返回正文)。 */
export interface ResearchReferenceInfo {
  /** 安全文件名(作为 reference_files 的取值) */
  name: string;
  /** docx / xlsx / pdf / text */
  kind: string;
  /** 提取字符数 */
  chars: number;
  /** 前 200 字预览 */
  preview: string;
}

/** 导入一份研究参考资料(前端读文件为 base64 后调用;仅支持文本类格式)。 */
export const importResearchReference = (fileName: string, dataBase64: string) =>
  invoke<ResearchReferenceInfo>('import_research_reference', {
    fileName,
    dataBase64,
  });

/** 列出本机已导入的参考资料(字符数取提取缓存长度)。 */
export const listResearchReferences = () =>
  invoke<ResearchReferenceInfo[]>('list_research_references');

/** 删除一份已导入的研究参考资料(原始文件 + 提取缓存)。 */
export const deleteResearchReference = (name: string) =>
  invoke<void>('delete_research_reference', { name });

/** 原生「另存为」对话框结果。existed=选择时同名文件已存在(系统对话框已完成覆盖确认)。 */
export interface SavePathPick {
  path: string;
  existed: boolean;
  /** N-6:后端签发的路径令牌,导出命令凭令牌写盘(渲染层无法伪造任意路径)。 */
  token: string;
}

/** 原生「另存为」对话框(报告导出);取消返回 null。 */
export const pickSavePath = (
  title: string,
  defaultName: string,
  filterName: string,
  filterExt: string,
) =>
  invoke<SavePathPick | null>('pick_save_path', {
    title,
    defaultName,
    filterName,
    filterExt,
  });

// ============ D4:PDF(打印对话框) ============

/** 唤起主窗口的原生打印对话框(用户在对话框里选「另存为 PDF」)。
 *  调用前渲染层需先把待导出内容铺成全屏白底覆盖层。 */
export const exportPdf = () => invoke<void>('export_pdf');
