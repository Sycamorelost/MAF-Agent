import { invoke } from '@tauri-apps/api/core';

// ============ 通用导出(P1 自 ipc/research 迁出,IPC 名不变) ============

/** 后端 H-2:目标已存在但未确认覆盖时返回的错误码。 */
export const EXPORT_TARGET_EXISTS = 'TARGET_EXISTS';

/** 原生「另存为」对话框结果。existed=选择时同名文件已存在(系统对话框已完成覆盖确认)。 */
export interface SavePathPick {
  path: string;
  existed: boolean;
  /** N-6:后端签发的路径令牌,导出命令凭令牌写盘(渲染层无法伪造任意路径)。 */
  token: string;
}

/** 原生「另存为」对话框;取消返回 null。 */
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

/** 唤起主窗口的原生打印对话框(用户在对话框里选「另存为 PDF」)。
 *  调用前渲染层需先把待导出内容铺成全屏白底覆盖层。 */
export const exportPdf = () => invoke<void>('export_pdf');
