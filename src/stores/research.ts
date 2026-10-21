//! 研究状态(Phase 2):按职责拆分为 session / trail 子模块。
//! 本文件仅做聚合导出,保持既有 `import ... from '.../stores/research'` 路径不变。

export { useResearchStore } from './research/session';
export type { ActiveResearch } from './research/trail';
