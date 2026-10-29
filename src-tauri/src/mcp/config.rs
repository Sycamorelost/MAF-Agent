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
    /// 文件系统类工具:显式允许的目录列表(空 = 不做文件夹授权限制)
    #[serde(default)]
    pub allowed_dirs: Vec<PathBuf>,
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
    /// 对话工具调用总开关(应用级)。关闭时不向模型下发任何工具定义,
    /// 每次请求可省去数千 token 的工具 schema,显著降低长会话首字延迟。
    /// 默认关闭:纯对话场景不需要工具;需要文件/命令/联网能力时在设置页打开。
    #[serde(default)]
    pub tools_enabled: bool,
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
/// 全部 `enabled: true` —— 开箱即用(v2);安全边界由审批弹窗、目录白名单与审计兜底。
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
            // 允许目录为空时不做文件夹授权限制:启动时传入本机全部磁盘根(见 launch_dirs);
            // 用户也可在 UI 里指定目录主动收窄范围
            allowed_dirs: vec![],
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

/// 用户显式配置的允许目录;空 = 不做文件夹授权限制(安全预检放行全部位置)。
pub fn effective_allowed_dirs(cfg: &McpServerConfig) -> Vec<PathBuf> {
    cfg.allowed_dirs.clone()
}

/// 启动文件系统 server 时传入的目录参数:
/// 未配置 → 本机全部磁盘根(不设沙箱);已配置 → 只传这些目录,主动收窄范围。
pub fn launch_dirs(cfg: &McpServerConfig) -> Vec<PathBuf> {
    if !is_filesystem_server(cfg) {
        return cfg.allowed_dirs.clone();
    }
    if cfg.allowed_dirs.is_empty() {
        disk_roots()
    } else {
        cfg.allowed_dirs.clone()
    }
}

/// 本机全部磁盘根(Windows 逐盘符探测,Unix 用 `/`)。
/// 盘符根用正斜杠形式(`C:/`):结尾的 `\` 在经 cmd.exe 转义时会吃掉后面的引号,
/// 导致多个盘符参数粘连成一个(`C:\"; D:\`),server 会判定目录不可访问。
fn disk_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut out: Vec<PathBuf> = ('A'..='Z')
            .map(|c| PathBuf::from(format!("{c}:/")))
            .filter(|p| p.exists())
            .collect();
        if out.is_empty() {
            out.push(PathBuf::from("C:/"));
        }
        out
    }
    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
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
        tools_enabled: false,
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
        }
    }

    #[test]
    fn filesystem_default_launch_covers_all_drives() {
        let dirs = launch_dirs(&fs_server(vec![]));
        assert!(!dirs.is_empty(), "无沙箱:未配置时必须给出磁盘根");
        #[cfg(windows)]
        assert!(
            dirs.iter().any(|d| d.to_string_lossy().starts_with("C:/")),
            "实际:{dirs:?}"
        );
        // 盘符根不得以 `\` 结尾(arg 经 cmd.exe 转义会把后续引号吃掉)
        assert!(
            dirs.iter().all(|d| !d.to_string_lossy().ends_with('\\')),
            "实际:{dirs:?}"
        );
    }

    #[test]
    fn filesystem_user_dirs_take_precedence() {
        let mine = vec![
            PathBuf::from("D:\\work"),
            PathBuf::from("C:\\Users\\me\\docs"),
        ];
        assert_eq!(effective_allowed_dirs(&fs_server(mine.clone())), mine);
        assert_eq!(launch_dirs(&fs_server(mine.clone())), mine);
    }

    #[test]
    fn filesystem_without_dirs_has_no_precheck_scope() {
        assert!(
            effective_allowed_dirs(&fs_server(vec![])).is_empty(),
            "未配置目录时预检不应限制位置"
        );
    }

    #[test]
    fn non_filesystem_server_never_gets_dirs() {
        let mut mem = fs_server(vec![]);
        mem.args = vec!["-y".into(), "@modelcontextprotocol/server-memory".into()];
        assert!(launch_dirs(&mem).is_empty());
        assert!(effective_allowed_dirs(&mem).is_empty());
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
