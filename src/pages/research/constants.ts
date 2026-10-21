export const STATUS_LABEL: Record<string, string> = {
  running: '运行中',
  paused: '已暂停',
  done: '已完成',
  error: '失败',
  cancelled: '已终止',
};

export const DEPTH_PRESETS = [
  { label: '快速', rounds: 4 },
  { label: '标准', rounds: 8 },
  { label: '深度', rounds: 15 },
];

export const SOURCE_MODES = [
  { id: 'web', label: '全网' },
  { id: 'academic', label: '学术优先' },
  { id: 'whitelist', label: '域名白名单' },
];

