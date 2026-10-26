//! 工具调用痕迹 —— 正文里只用一行小字说明「做了什么」,点开才展开完整输出。
//! 数据由 useChatSend 注入为 ```maf-tool 围栏块(单行 JSON),持久化后仍按此渲染。

import { useState } from 'react';
import { IconChevronRight } from './icons';

type Trace = { server: string; tool: string; ok: boolean; content: string };

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

function parse(raw: string): Trace | null {
  try {
    const v = JSON.parse(raw) as { s?: unknown; t?: unknown; ok?: unknown; c?: unknown };
    if (typeof v.t !== 'string' || !v.t) return null;
    return {
      server: typeof v.s === 'string' ? v.s : '',
      tool: v.t,
      ok: v.ok !== false,
      content: typeof v.c === 'string' ? v.c : '',
    };
  } catch {
    return null;
  }
}

export function ToolTrace({ raw }: { raw: string }) {
  const [open, setOpen] = useState(false);
  const t = parse(raw);
  // 流式期间的残缺块:只留占位,不把半截 JSON 暴露给用户
  if (!t) return <div className="tool-trace pending">工具操作进行中…</div>;
  const label = ACTION[t.tool] ?? t.tool;
  return (
    <div className={`tool-trace${open ? ' open' : ''}${t.ok ? '' : ' err'}`}>
      <button
        type="button"
        className="tool-trace-head"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        title={t.server ? `${t.server} / ${t.tool}` : t.tool}
      >
        <IconChevronRight size={12} />
        <span className="tool-trace-label">{label}</span>
        <span className="tool-trace-tool">{t.tool}</span>
        {!t.ok && <span className="tool-trace-bad">失败</span>}
        <span className="tool-trace-hint">
          {!t.content ? '无输出' : open ? '收起' : '查看结果'}
        </span>
      </button>
      {open && t.content && <pre className="tool-trace-body">{t.content}</pre>}
    </div>
  );
}
