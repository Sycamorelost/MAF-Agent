//! MCP 工具说明目录 —— 已知工具的中文一句话解释。
//! 服务器自己给的 description 是英文长文,列表里只显示这里的中文摘要;
//! 未知工具回退到服务器原文。

/** 预置 server 的作用说明(按配置里的 name 匹配)。 */
export const SERVER_DESC: Record<string, string> = {
  项目文件: "读写本机文件与文件夹;未配置目录时不设沙箱(整机所有磁盘),可点「选择目录…」主动收窄范围",
  长期记忆:
    "跨会话知识图谱(实体+关系);会话摘要/偏好/待办由内置 memory.md 自动维护,两者分工不重复",
  顺序思考: "让模型分步推理,每一步都可以回头修订",
};

/** 工具名 → 中文作用说明。 */
const TOOL_DESC: Record<string, string> = {
  // ---- 文件系统 @modelcontextprotocol/server-filesystem ----
  read_file: "读取文本文件内容(旧接口,已不推荐,用 read_text_file)",
  read_text_file: "读取一个文本文件的完整内容",
  read_media_file: "读取图片/音频等文件并返回 base64",
  read_multiple_files: "一次读取多个文本文件",
  write_file: "新建或整体覆盖写入一个文件",
  edit_file: "按文本片段做局部替换编辑",
  create_directory: "新建目录(自动创建父目录)",
  list_directory: "列出目录的直接子项",
  list_directory_with_sizes: "列出目录子项并附带大小",
  directory_tree: "递归输出目录树结构",
  move_file: "移动或重命名文件/目录",
  search_files: "按名称或内容递归搜索文件",
  get_file_info: "查看单个文件的元信息(大小/时间/权限)",
  list_allowed_directories: "列出当前允许访问的目录",

  // ---- 长期记忆 @modelcontextprotocol/server-memory ----
  create_entities: "新建实体节点(人物/项目/概念等)",
  create_relations: "在两个实体之间建立关系",
  add_observations: "给已有实体追加观察记录",
  delete_entities: "删除实体及其相关关系",
  delete_observations: "删除实体下的指定观察",
  delete_relations: "删除指定的实体关系",
  read_graph: "读取整张知识图谱",
  search_nodes: "按关键词搜索实体",
  open_nodes: "按名称打开指定实体",

  // ---- 顺序思考 @modelcontextprotocol/server-sequential-thinking ----
  sequentialthinking: "分步推理:输出、修订和分支思考步骤",
};

/** 工具说明:优先中文目录,其次服务器原文,最后空串。 */
export function toolDesc(name: string, fallback?: string): string {
  return TOOL_DESC[name] ?? fallback ?? "";
}

/** server 说明:预置项有中文解释,自定义项返回空串。 */
export function serverDesc(name: string): string {
  return SERVER_DESC[name] ?? "";
}
