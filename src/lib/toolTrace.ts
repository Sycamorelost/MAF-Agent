//! 工具调用痕迹共用解析(P6):对话内 ToolTrace 与工作台检查器(工具流水/产物/子任务卡片)共用。
//! 消息正文里的 ```maf-tool 围栏块为单行 JSON:{s,t,ok,c,a?,ms?,tid?}
//! (a=参数摘要,ms=耗时,tid=关联任务 id,val.md §5.4 可选字段;旧块兼容缺省)。

import type { MessageRow } from './ipc/types';

export interface Trace {
  server: string;
  tool: string;
  ok: boolean;
  content: string;
  /** 实际执行参数摘要(JSON,可空;旧块无此字段) */
  args: string | null;
  /** 执行耗时(毫秒,不含审批等待;旧块无此字段) */
  elapsedMs: number | null;
  /** 关联的目标级任务 id(发送时若有 running 任务则打上;旧块无此字段) */
  taskId: string | null;
}

/** 工具名 → 动作短语(未知工具回退工具名)。 */
const ACTION: Record<string, string> = {
  // 内置:文件
  fs_read_text: '读取文件',
  fs_write_text: '写入文件',
  fs_list: '列出目录',
  fs_delete: '删除文件',
  fs_move: '移动文件',
  fs_backup_list: '查看备份',
  fs_backup_restore: '恢复备份',
  fs_export_docx: '导出文档',
  fs_export_xlsx: '导出表格',
  // 内置:执行与系统
  shell_exec: '执行命令',
  sys_overview: '查看系统概况',
  sys_processes: '查看进程',
  sys_disks: '查看磁盘',
  sys_networks: '查看网络',
  get_now: '查询当前时间',
  // 内置:GUI
  ui_list_windows: '枚举窗口',
  ui_find_elements: '查找界面元素',
  ui_click: '点击界面元素',
  ui_type: '输入文本',
  // 内置:任务与委派
  todo_write: '更新任务清单',
  todo_read: '读取任务清单',
  delegate: '委派子任务',
  // 内置:压缩包
  zip_extract: '解压文件',
  zip_create: '创建压缩包',
  // MCP filesystem
  read_text_file: '读取文件',
  read_multiple_files: '读取多个文件',
  write_file: '写入文件',
  edit_file: '编辑文件',
  create_directory: '新建目录',
  list_directory: '列出目录',
  list_directory_with_sizes: '列出目录',
  directory_tree: '查看目录树',
  move_file: '移动文件',
  search_files: '搜索文件',
  get_file_info: '查看文件信息',
  list_allowed_directories: '查看允许目录',
  // MCP memory
  create_entities: '写入记忆实体',
  create_relations: '写入记忆关系',
  add_observations: '追加记忆',
  delete_entities: '删除记忆实体',
  delete_observations: '删除记忆',
  delete_relations: '删除记忆关系',
  read_graph: '读取记忆图谱',
  search_nodes: '搜索记忆',
  open_nodes: '打开记忆节点',
  // MCP sequential thinking
  sequentialthinking: '分步推理',
};

export function actionLabel(tool: string): string {
  return ACTION[tool] ?? tool;
}

/** 解析单条 maf-tool 块内容(单行 JSON);残缺/旧格式返回 null 或缺省字段。 */
export function parseTrace(raw: string): Trace | null {
  try {
    const v = JSON.parse(raw) as {
      s?: unknown; t?: unknown; ok?: unknown; c?: unknown; a?: unknown; ms?: unknown; tid?: unknown;
    };
    if (typeof v.t !== 'string' || !v.t) return null;
    return {
      server: typeof v.s === 'string' ? v.s : '',
      tool: v.t,
      ok: v.ok !== false,
      content: typeof v.c === 'string' ? v.c : '',
      args: typeof v.a === 'string' ? v.a : null,
      elapsedMs: typeof v.ms === 'number' && Number.isFinite(v.ms) ? v.ms : null,
      taskId: typeof v.tid === 'string' && v.tid ? v.tid : null,
    };
  } catch {
    return null;
  }
}

/** 正文段落:普通 Markdown 文本 / maf-tool 工具块(供消息气泡分段渲染)。 */
export type ContentSegment =
  | { kind: 'text'; text: string }
  | { kind: 'trace'; raw: string };

/**
 * 按 maf-tool 围栏切分正文:文本段落留在气泡内,工具块提到气泡外(对齐设计稿层级)。
 * 流式未闭合的围栏按工具块处理,避免把半截 JSON 当正文展示。
 */
export function splitTraceSegments(content: string): ContentSegment[] {
  const re = /```maf-tool\n([\s\S]*?)(?:\n```|$)/g;
  const out: ContentSegment[] = [];
  let last = 0;
  for (const m of content.matchAll(re)) {
    const start = m.index ?? 0;
    if (start > last) out.push({ kind: 'text', text: content.slice(last, start) });
    out.push({ kind: 'trace', raw: m[1].trim() });
    last = start + m[0].length;
  }
  if (last < content.length) out.push({ kind: 'text', text: content.slice(last) });
  return out.filter((s) => s.kind === 'trace' || s.text.trim().length > 0);
}

/** 工具参数里最适合展示的字段(路径 / 查询 / 命令等),缺省取原始文本。 */
export function argSummary(trace: Trace): string {
  const keys = ['path', 'query', 'q', 'pattern', 'command', 'cmd', 'url', 'name', 'task'];
  if (trace.args) {
    try {
      const v = JSON.parse(trace.args) as Record<string, unknown>;
      for (const k of keys) {
        const x = v[k];
        if (typeof x === 'string' && x.trim()) return oneLine(x, 72);
      }
    } catch {
      return oneLine(trace.args, 72);
    }
  }
  return '';
}

function oneLine(s: string, max: number): string {
  const flat = s.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

const BLOCK_RE = /```maf-tool\n([\s\S]*?)\n```/g;

/** 从一段正文里提取全部 maf-tool 块(raw JSON 字符串,按出现顺序)。 */
export function extractTraceBlocks(content: string): string[] {
  const out: string[] = [];
  for (const m of content.matchAll(BLOCK_RE)) out.push(m[1]);
  return out;
}

/** 会话级工具痕迹条目(绑定来源消息,供工具流水跳转)。 */
export interface SessionTrace {
  messageId: number;
  created: string;
  trace: Trace;
}

/** 按消息序汇总全部工具痕迹(流式文本可作为伪消息追加,id=-1)。 */
export function collectSessionTraces(
  messages: Pick<MessageRow, 'id' | 'created' | 'content'>[],
): SessionTrace[] {
  const out: SessionTrace[] = [];
  for (const m of messages) {
    for (const raw of extractTraceBlocks(m.content)) {
      const trace = parseTrace(raw);
      if (trace) out.push({ messageId: m.id, created: m.created, trace });
    }
  }
  return out;
}

/** 产物条目:会话附件 + 工具写出的工作区文件(fs_write_text / fs_export_*)。 */
export interface Artifact {
  kind: 'attachment' | 'file';
  name: string;
  /** attachment=附件存储路径(预览用);file=工作区相对路径(定位用) */
  path: string;
  size: number | null;
  messageId: number;
  created: string;
  isImage: boolean;
  /** 第几步生成(按会话内工具调用序号;用户附件为 null) */
  step: number | null;
}

const WRITE_TOOLS = new Set(['fs_write_text', 'fs_write', 'fs_export_docx', 'fs_export_xlsx', 'write_file']);
const IMAGE_EXTS = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;

function baseName(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

/** 从消息 JSON 参数里取写文件路径(缺省 null)。 */
function writePathOf(args: string | null): string | null {
  if (!args) return null;
  try {
    const v = JSON.parse(args) as { path?: unknown };
    return typeof v.path === 'string' && v.path ? v.path : null;
  } catch {
    return null;
  }
}

/** 汇总会话产物:附件(user 消息 attachments JSON)+ 工具写出的工作区文件。 */
export function extractArtifacts(
  messages: Pick<MessageRow, 'id' | 'created' | 'content' | 'attachments'>[],
): Artifact[] {
  const out: Artifact[] = [];
  /** 会话内工具调用序号:产物据此标注「第 N 步生成」。 */
  let step = 0;
  for (const m of messages) {
    if (m.attachments) {
      try {
        const arr = JSON.parse(m.attachments) as {
          name?: unknown; path?: unknown; size?: unknown; kind?: unknown;
        }[];
        if (Array.isArray(arr)) {
          for (const a of arr) {
            if (typeof a.name !== 'string' || typeof a.path !== 'string') continue;
            out.push({
              kind: 'attachment',
              name: a.name,
              path: a.path,
              size: typeof a.size === 'number' ? a.size : null,
              messageId: m.id,
              created: m.created,
              isImage: a.kind === 'image' || IMAGE_EXTS.test(a.name),
              step: null,
            });
          }
        }
      } catch {
        /* 附件 JSON 损坏则跳过 */
      }
    }
    for (const raw of extractTraceBlocks(m.content)) {
      step += 1;
      const trace = parseTrace(raw);
      if (!trace || !trace.ok || !WRITE_TOOLS.has(trace.tool)) continue;
      const p = writePathOf(trace.args);
      if (!p) continue;
      out.push({
        kind: 'file',
        name: baseName(p),
        path: p,
        size: null,
        messageId: m.id,
        created: m.created,
        isImage: IMAGE_EXTS.test(p),
        step,
      });
    }
  }
  // 同一路径只保留最近一次产出
  const lastIndex = new Map<string, number>();
  out.forEach((a, i) => lastIndex.set(`${a.kind}:${a.path}`, i));
  return out.filter((a, i) => lastIndex.get(`${a.kind}:${a.path}`) === i);
}

/** 运行时把流式增量里的写文件工具块合并进产物列表(与 traces 同链路,实时更新产物面板)。 */
export function mergeArtifacts(base: Artifact[], streaming: string): Artifact[] {
  if (!streaming) return base;
  const stepOffset = base.length > 0
    ? Math.max(...base.map((a) => a.step ?? 0)) + 1
    : 1;
  const extra: Artifact[] = [];
  let i = 0;
  for (const raw of extractTraceBlocks(streaming)) {
    i += 1;
    const trace = parseTrace(raw);
    if (!trace || !trace.ok || !WRITE_TOOLS.has(trace.tool)) continue;
    const p = writePathOf(trace.args);
    if (!p) continue;
    extra.push({
      kind: 'file',
      name: baseName(p),
      path: p,
      size: null,
      messageId: -1,
      created: '',
      isImage: IMAGE_EXTS.test(p),
      step: stepOffset + i - 1,
    });
  }
  if (extra.length === 0) return base;
  // 同一路径去重:流式里同一路径多次写取后一次
  const merged = [...base, ...extra];
  const lastIndex = new Map<string, number>();
  merged.forEach((a, idx) => lastIndex.set(`${a.kind}:${a.path}`, idx));
  return merged.filter((a, idx) => lastIndex.get(`${a.kind}:${a.path}`) === idx);
}

/** 子智能体(delegate)结果解析:成功「【子智能体「X」的执行结果】\n...」,失败「子智能体「X」执行失败:...」。 */
export function parseDelegate(trace: Trace): { agent: string; summary: string } {
  const m = /^【子智能体「(.+?)」的执行结果】\n?/.exec(trace.content);
  if (m) return { agent: m[1], summary: trace.content.slice(m[0].length) };
  const f = /^子智能体「(.+?)」/.exec(trace.content);
  if (f) return { agent: f[1], summary: trace.content };
  return { agent: '', summary: trace.content };
}

/** 从 delegate 参数 JSON 里取子任务描述(缺省 null)。 */
export function delegateTaskOf(args: string | null): string | null {
  if (!args) return null;
  try {
    const v = JSON.parse(args) as { task?: unknown };
    return typeof v.task === 'string' && v.task ? v.task : null;
  } catch {
    return null;
  }
}

/** 耗时格式化:<1s 显示毫秒,否则秒(一位小数)。 */
export function fmtMs(ms: number | null): string {
  if (ms == null) return '';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** 文件大小格式化。 */
export function fmtBytes(n: number | null): string {
  if (n == null) return '';
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / 1024 / 1024).toFixed(1)}MB`;
}
