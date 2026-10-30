//! 内置四档运行时预设(val.md §4.5.5 采纳矩阵),只读;用户覆盖走 `<data_dir>/presets.json`。
//!
//! 工具名为 `mcp_builtin_tools` 的实际工具名(include/exclude 支持 `*`);
//! MCP server 工具与内置工具共用同一名单逻辑(commands/mcp.rs 下发前过滤)。

use std::sync::LazyLock;

use super::{Preset, PresetsFile};

/// 内置预设清单 JSON(结构对齐 val.md §5.8;presets_version 升级即新版本)。
const BUILTIN_JSON: &str = r#"{
  "presets_version": 1,
  "presets": [
    {
      "id": "economy",
      "name": "经济",
      "order": 1,
      "description": "最少工具与上下文:只读检索 + shell,适合快问快答与弱模型。",
      "tools": {
        "include": ["get_now", "fs_read", "fs_list", "doc_search", "doc_read", "shell_exec"],
        "exclude": ["*"]
      },
      "context": { "max_messages": 20, "max_chars": 24000, "prune": { "threshold_chars": 8192, "head": 4096, "tail": 1024 } },
      "memory": { "strategy": "summary", "max_tokens": 500 },
      "approval": "ask",
      "retrieval": { "lexical": "trigram3", "vector": false, "rerank": false, "top_k": 3 }
    },
    {
      "id": "standard",
      "name": "标准",
      "order": 2,
      "description": "日常办公与写作的默认档:读写文件、任务清单、混合检索。",
      "tools": {
        "include": ["get_now", "fs_read", "fs_list", "fs_grep", "fs_write", "file_extract", "doc_search", "doc_read", "zip_list", "todo_write", "todo_read", "shell_exec"],
        "exclude": ["*"]
      },
      "context": { "max_messages": 40, "max_chars": 48000, "prune": { "threshold_chars": 8192, "head": 4096, "tail": 1024 } },
      "memory": { "strategy": "hybrid", "max_tokens": 800 },
      "approval": "workspace",
      "retrieval": { "lexical": "trigram3", "vector": true, "rerank": false, "top_k": 5 }
    },
    {
      "id": "deep",
      "name": "深度",
      "order": 3,
      "description": "长文档与多步调研:子代理隔离重活,检索加重排,逐次审批。",
      "tools": { "include": ["*"], "exclude": [] },
      "context": { "max_messages": 30, "max_chars": 48000, "prune": { "threshold_chars": 8192, "head": 4096, "tail": 1024 } },
      "memory": { "strategy": "hybrid", "max_tokens": 800 },
      "approval": "ask",
      "retrieval": { "lexical": "trigram3", "vector": true, "rerank": true, "top_k": 5 }
    },
    {
      "id": "code",
      "name": "代码",
      "order": 4,
      "description": "改码与重构:全量工具、计划清单与更大上下文。",
      "tools": { "include": ["*"], "exclude": [] },
      "context": { "max_messages": 40, "max_chars": 64000, "prune": { "threshold_chars": 8192, "head": 4096, "tail": 1024 } },
      "memory": { "strategy": "full", "max_tokens": 1000 },
      "approval": "workspace",
      "retrieval": { "lexical": "trigram3", "vector": false, "rerank": false, "top_k": 5 }
    }
  ]
}"#;

static BUILTIN: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    let file: PresetsFile = serde_json::from_str(BUILTIN_JSON).expect("内置预设 JSON 必须可解析");
    file.presets
});

/// 内置四档(克隆返回,调用方可安全修改)。
pub fn presets() -> Vec<Preset> {
    BUILTIN.clone()
}
