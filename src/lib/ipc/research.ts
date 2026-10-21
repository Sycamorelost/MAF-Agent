//! 深度研究 IPC(Phase 2):按职责拆分为 session / template / export 子模块。
//! 本文件仅做聚合导出,保持既有 `import ... from '.../lib/ipc/research'` 路径不变。

export * from './research/session';
export * from './research/template';
export * from './research/export';
