//! MCP Server 配置加载/保存 —— 从 settings.json 的 `mcp_servers` 字段读写。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 传输方式:stdio(本地子进程)/ http(远端 Streamable HTTP)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Stdio,
    Http,
}

/// 单个 MCP Server 配置 —— 用户在设置页添加/编辑。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 内部唯一标识,UI 显示名
    pub name: String,
    pub enabled: bool,
    /// 传输方式
    pub transport: Transport,
    /// 要 spawn 的可执行程序(如 "npx" 或 "node"),仅 stdio 使用
    #[serde(default)]
    pub command: String,
    /// 传递给 command 的参数数组,仅 stdio 使用
    #[serde(default)]
    pub args: Vec<String>,
    /// 远端 MCP 端点(Streamable HTTP),仅 http 使用
    #[serde(default)]
    pub url: String,
    /// http 传输的附加请求头(如 Authorization / X-API-Key),明文存放在配置文件
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 文件系统类工具:显式允许的目录列表
    #[serde(default)]
    pub allowed_dirs: Vec<PathBuf>,
    /// 路径/参数中命中以下模式时直接拒绝
    #[serde(default)]
    pub deny_patterns: Vec<String>,
}

impl McpServerConfig {
    pub fn is_http(&self) -> bool {
        matches!(self.transport, Transport::Http)
    }
}

/// 顶层配置 —— 与 settings.json 其他字段平级。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
    /// Phase 6:是否向模型暴露内置工具(get_now / shell_exec / fs_* / zip_* 等)。
    /// 默认 true:开箱即用,对话无需额外配置即可调用工具。
    /// 极少数 OpenAI 兼容中转站不接受 `tools` 字段会 400,此时可在设置里关掉。
    #[serde(default = "default_true")]
    pub builtin_tools: bool,
    /// 预置服务已合并到哪一版。旧配置文件为 0,加载时据此补齐新增的预置项(只补一次)。
    #[serde(default)]
    pub presets_version: u32,
    /// 下载资源优先使用国内镜像(npx→npmmirror,uvx/pip→清华 PyPI),默认开启保证国内稳定性。
    #[serde(default = "default_true")]
    pub china_mirror: bool,
}

fn default_true() -> bool {
    true
}

/// 当前预置服务版本 —— 新增/修改预置项时 +1。
/// v2:预置 server 由默认关闭改为默认启用(开箱即用),旧配置合并时一次性置为启用。
/// v3:builtin_tools 默认开启(开箱即用,对话直接可用 shell_exec/fs_*),旧配置合并时一次性点亮。
pub const PRESETS_VERSION: u32 = 3;

/// 预置服务清单(官方 npm 包,stdio,无需 API Key)。
/// 全部 `enabled: true` —— 开箱即用(v2);安全边界由审批弹窗、目录白名单、敏感黑名单与审计兜底。
pub fn preset_servers() -> Vec<McpServerConfig> {
    vec![
        McpServerConfig {
            name: "项目文件".into(),
            enabled: true,
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
            ],
            url: String::new(),
            headers: HashMap::new(),
            // 允许目录为空时回落到沙箱目录 <data_dir>/mcp-sandbox(见 effective_allowed_dirs);
            // 用户可在 UI 里指定任意目录,全盘读写能力由审批弹窗 + 敏感文件黑名单兜底
            allowed_dirs: vec![],
            deny_patterns: vec![
                ".env".into(),
                "*.pem".into(),
                "*id_rsa*".into(),
                "*credentials*.json".into(),
            ],
        },
        McpServerConfig {
            name: "长期记忆".into(),
            enabled: true,
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
            url: String::new(),
            headers: HashMap::new(),
            allowed_dirs: vec![],
            deny_patterns: vec![],
        },
        McpServerConfig {
            name: "顺序思考".into(),
            enabled: true,
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-sequential-thinking".into(),
            ],
            url: String::new(),
            headers: HashMap::new(),
            allowed_dirs: vec![],
            deny_patterns: vec![],
        },
    ]
}

impl McpConfig {
    #[allow(dead_code)] // Phase 5d 设置页显示时用
    pub fn enabled_servers(&self) -> impl Iterator<Item = &McpServerConfig> {
        self.servers.iter().filter(|s| s.enabled)
    }
}

/// memory server 默认把知识图谱写在 npx 包目录里,缓存清理即丢失。
/// 启动时注入 MEMORY_FILE_PATH,落到应用数据目录。
pub(crate) fn memory_env(args: &[String], data_dir: &Path) -> Vec<(String, String)> {
    if args.iter().any(|a| a.contains("server-memory")) {
        vec![(
            "MEMORY_FILE_PATH".into(),
            data_dir.join("mcp-memory.jsonl").display().to_string(),
        )]
    } else {
        Vec::new()
    }
}

/// 是否文件系统类 server(以官方 server-filesystem 包为准)。
pub fn is_filesystem_server(cfg: &McpServerConfig) -> bool {
    cfg.args.iter().any(|a| a.contains("server-filesystem"))
}

/// 文件系统 server 的有效允许目录。
/// 用户未指定时回落到沙箱目录 `<data_dir>/mcp-sandbox`(默认仅可读写沙箱);
/// 用户可指定任意目录(可读写全盘),越权风险由审批弹窗与敏感文件黑名单兜底。
/// 启动参数与安全预检必须共用此函数,保证两侧边界一致。
pub fn effective_allowed_dirs(cfg: &McpServerConfig, data_dir: &Path) -> Vec<PathBuf> {
    if is_filesystem_server(cfg) && cfg.allowed_dirs.is_empty() {
        vec![data_dir.join("mcp-sandbox")]
    } else {
        cfg.allowed_dirs.clone()
    }
}

/// 国内镜像环境变量:npx 首次下载走 npmmirror,uvx/pip 走清华 PyPI。
/// 按启动命令识别包管理器;只在 china_mirror 开启时由调用方注入。
pub fn mirror_env(command: &str) -> Vec<(String, String)> {
    let cmd = command.to_lowercase();
    let mut env = Vec::new();
    if cmd.contains("npx") || cmd.contains("npm") {
        env.push((
            "NPM_CONFIG_REGISTRY".into(),
            "https://registry.npmmirror.com".into(),
        ));
    }
    if cmd.contains("uvx") || cmd == "uv" {
        env.push((
            "UV_DEFAULT_INDEX".into(),
            "https://pypi.tuna.tsinghua.edu.cn/simple".into(),
        ));
    }
    if cmd.contains("pip") {
        env.push((
            "PIP_INDEX_URL".into(),
            "https://pypi.tuna.tsinghua.edu.cn/simple".into(),
        ));
    }
    env
}

/// 内置默认 Server 集合(首次启动或用户点"重置 MCP"时写入)。
pub fn default_servers() -> McpConfig {
    McpConfig {
        servers: preset_servers(),
        builtin_tools: true,
        presets_version: PRESETS_VERSION,
        china_mirror: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs_server(dirs: Vec<PathBuf>) -> McpServerConfig {
        McpServerConfig {
            name: "项目文件".into(),
            enabled: true,
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
            ],
            url: String::new(),
            headers: HashMap::new(),
            allowed_dirs: dirs,
            deny_patterns: vec![],
        }
    }

    #[test]
    fn filesystem_empty_dirs_falls_back_to_sandbox() {
        let data = Path::new("C:\\data");
        let dirs = effective_allowed_dirs(&fs_server(vec![]), data);
        assert_eq!(dirs, vec![data.join("mcp-sandbox")]);
    }

    #[test]
    fn filesystem_user_dirs_take_precedence() {
        let data = Path::new("C:\\data");
        let mine = vec![
            PathBuf::from("D:\\work"),
            PathBuf::from("C:\\Users\\me\\docs"),
        ];
        let dirs = effective_allowed_dirs(&fs_server(mine.clone()), data);
        assert_eq!(dirs, mine);
    }

    #[test]
    fn non_filesystem_server_never_gets_sandbox() {
        let data = Path::new("C:\\data");
        let mut mem = fs_server(vec![]);
        mem.args = vec!["-y".into(), "@modelcontextprotocol/server-memory".into()];
        assert!(effective_allowed_dirs(&mem, data).is_empty());
    }

    #[test]
    fn mirror_env_routes_package_managers_to_china_mirrors() {
        assert!(
            mirror_env("npx")
                .iter()
                .any(|(k, v)| k == "NPM_CONFIG_REGISTRY" && v.contains("npmmirror"))
        );
        assert!(
            mirror_env("uvx")
                .iter()
                .any(|(k, v)| k == "UV_DEFAULT_INDEX" && v.contains("tsinghua"))
        );
        assert!(
            mirror_env("pip")
                .iter()
                .any(|(k, v)| k == "PIP_INDEX_URL" && v.contains("tsinghua"))
        );
        assert!(mirror_env("node").is_empty());
    }
}
