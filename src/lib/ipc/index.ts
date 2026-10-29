//! Tauri IPC 封装,统一类型与错误处理。
//!
//! 按领域拆分为子模块,本文件仅做聚合导出,保持 `import * as ipc from './ipc'` 兼容。

export * from './types';
export * from './session';
export * from './message';
export * from './attachment';
export * from './doc';
export * from './memory';
export * from './provider';
export * from './research';
export * from './notes';
export * from './fs';
export * from './media';
export * from './tutorial';
export * from './aiflavor';
