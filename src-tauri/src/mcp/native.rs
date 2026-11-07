//! P0b(A1/A2/C5):预置 MCP server 的「原生二进制」启动层。
//!
//! 设计原则: **默认行为零变化**。
//! 只有同时满足下面两条,才会用原生二进制替代 npx:
//! 1. 环境变量 `MYAGENT_MCP_NATIVE=1`(显式 opt-in,默认关闭);
//! 2. 资源目录里存在 `manifest.json`,且声明的二进制文件真实存在。
//!
//! 任何一条不满足 → 原样返回调用方给的 npx 命令与参数(与改造前完全一致)。
//!
//! 这样既能逐步消除 Node 依赖,又不会因为工具名/参数差异导致功能或权限范围变化 ——
//! 原生 server 的参数必须由 manifest 显式声明,代码绝不去猜测第三方 CLI 的旗标。
//!
//! 资源目录(按优先级,命中即停):
//! 1. `MYAGENT_MCP_BIN_DIR`(可多个目录,用平台路径分隔符连接)
//! 2. `<exe 目录>/resources/mcp/<平台标记>`
//! 3. `<exe 目录>/../Resources/mcp/<平台标记>`(macOS .app 内)
//!
//! `manifest.json` 示例:
//! ```json
//! {
//!   "filesystem": { "bin": "mcp-filesystem", "args": [], "pass_dirs": true },
//!   "memory":     { "bin": "mcp-memory-rs",  "args": [] }
//! }
//! ```
//! `pass_dirs` 为 true 时,把配置里 `allowed_dirs`(为空则是全盘根)追加为参数,
//! 与 npx 版的行为保持一致 —— 不收紧权限范围。

use std::path::{Path, PathBuf};

/// 显式开启原生后端的开关(默认关闭)。
pub const OPT_IN_ENV: &str = "MYAGENT_MCP_NATIVE";
/// 额外二进制目录覆盖(便于开发调试或用户自行放置)。
pub const BIN_DIR_ENV: &str = "MYAGENT_MCP_BIN_DIR";
const MANIFEST: &str = "manifest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Native,
    Npx,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Native => "native",
            Backend::Npx => "npx",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub command: String,
    pub args: Vec<String>,
    pub backend: Backend,
}

/// 是否显式开启原生后端(不认识的值一律视为关闭,避免误开启)。
pub fn is_enabled() -> bool {
    std::env::var(OPT_IN_ENV)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// 当前平台标记(`<os>-<arch>`),用于资源目录分层。
pub fn platform_tag() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(not(any(
        all(
            target_os = "windows",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "macos",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
    )))]
    {
        "unknown"
    }
}

/// 候选资源目录(去重、只保留存在的目录)。
pub fn bin_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(v) = std::env::var_os(BIN_DIR_ENV) {
        dirs.extend(std::env::split_paths(&v));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        dirs.push(parent.join("resources").join("mcp").join(platform_tag()));
        dirs.push(
            parent
                .join("..")
                .join("Resources")
                .join("mcp")
                .join(platform_tag()),
        );
    }
    let mut seen = std::collections::HashSet::new();
    dirs.into_iter()
        .filter(|d| seen.insert(d.clone()))
        .filter(|d| d.is_dir())
        .collect()
}

/// 预置 server 中文名 → manifest key。自定义 server 不参与原生替换。
fn preset_key(name: &str) -> Option<&'static str> {
    match name {
        "项目文件" => Some("filesystem"),
        "长期记忆" => Some("memory"),
        "顺序思考" => Some("sequential-thinking"),
        "联网搜索" => Some("web-search"),
        _ => None,
    }
}

/// 在指定目录里定位可执行文件(Windows 兼容 .exe/.cmd/.bat)。
fn find_executable(dir: &Path, bin: &str) -> Option<PathBuf> {
    let direct = dir.join(bin);
    if direct.is_file() {
        return Some(direct);
    }
    #[cfg(windows)]
    {
        for ext in ["exe", "cmd", "bat"] {
            let cand = dir.join(format!("{bin}.{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

fn read_manifest(dir: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 解析预置 server 的实际启动方式。
///
/// - `npx_command` / `npx_args` 是改造前原本要用的命令(兜底路径,逐字节保持不变);
/// - `dirs` 是配置解析出的目录参数(为空表示全盘根),仅当 manifest 声明 `pass_dirs` 时追加;
/// - `is_preset` 为 false(自定义 server)时永不替换,避免干扰用户自己的配置。
pub fn resolve(
    name: &str,
    npx_command: &str,
    npx_args: &[String],
    dirs: &[String],
    is_preset: bool,
) -> Launch {
    let fallback = || Launch {
        command: npx_command.to_string(),
        args: npx_args.to_vec(),
        backend: Backend::Npx,
    };

    if !is_preset || !is_enabled() {
        return fallback();
    }
    let Some(key) = preset_key(name) else {
        return fallback();
    };
    for dir in bin_dirs() {
        let Some(manifest) = read_manifest(&dir) else {
            continue;
        };
        let Some(entry) = manifest.get(key) else {
            continue;
        };
        let Some(bin) = entry.get("bin").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(path) = find_executable(&dir, bin) else {
            continue;
        };
        let mut args: Vec<String> = entry
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if entry
            .get("pass_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            args.extend(dirs.iter().cloned());
        }
        return Launch {
            command: path.display().to_string(),
            args,
            backend: Backend::Native,
        };
    }
    fallback()
}

/// 当前该 server 实际会用的后端(供 mcp_doctor 展示;http/自定义返回 "npx"/"http")。
pub fn describe_backend(
    name: &str,
    npx_command: &str,
    is_preset: bool,
    is_http: bool,
) -> &'static str {
    if is_http {
        return "http";
    }
    if !is_preset {
        return "npx";
    }
    let probe = resolve(name, npx_command, &[], &[], is_preset);
    probe.backend.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 环境变量是进程级全局状态,测试串行执行避免互相干扰。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d =
            std::env::temp_dir().join(format!("maf-native-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 默认(未 opt-in)必须得到与改造前完全一致的 npx 启动参数。
    #[test]
    fn default_stays_on_npx_byte_for_byte() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: 测试串行;仅影响本进程,且退出前恢复
        unsafe { std::env::remove_var(OPT_IN_ENV) };
        let args = vec![
            "-y".into(),
            "@modelcontextprotocol/server-filesystem".into(),
        ];
        let dirs = vec!["C:\\".to_string()];
        let l = resolve("项目文件", "npx", &args, &dirs, true);
        assert_eq!(l.backend, Backend::Npx);
        assert_eq!(l.command, "npx");
        assert_eq!(l.args, args, "默认路径不允许改写参数");
    }

    /// 自定义 server 永远不会被替换。
    #[test]
    fn custom_server_never_replaced() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var(OPT_IN_ENV, "1") };
        let dir = temp_dir("custom");
        std::fs::write(dir.join(MANIFEST), r#"{"my-server":{"bin":"whatever"}}"#).unwrap();
        unsafe { std::env::set_var(BIN_DIR_ENV, &dir) };
        let l = resolve("我的自定义", "npx", &["-y".into()], &[], false);
        assert_eq!(l.backend, Backend::Npx);
        unsafe { std::env::remove_var(BIN_DIR_ENV) };
        unsafe { std::env::remove_var(OPT_IN_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// opt-in 但资源目录里没有 manifest / 二进制 → 必须安全回退 npx。
    #[test]
    fn opt_in_without_binary_falls_back() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = temp_dir("empty");
        unsafe { std::env::set_var(OPT_IN_ENV, "1") };
        unsafe { std::env::set_var(BIN_DIR_ENV, &dir) };
        let args = vec!["-y".into(), "pkg".into()];
        let l = resolve("长期记忆", "npx", &args, &[], true);
        assert_eq!(l.backend, Backend::Npx);
        assert_eq!(l.args, args);
        unsafe { std::env::remove_var(BIN_DIR_ENV) };
        unsafe { std::env::remove_var(OPT_IN_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// manifest 命中的二进制存在 → 切原生,并按声明追加目录参数(权限范围不变)。
    #[test]
    fn opt_in_with_manifest_uses_native_and_passes_dirs() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = temp_dir("hit");
        let bin_name = if cfg!(windows) {
            "mcp-filesystem.exe"
        } else {
            "mcp-filesystem"
        };
        std::fs::write(dir.join(bin_name), b"stub").unwrap();
        std::fs::write(
            dir.join(MANIFEST),
            r#"{"filesystem":{"bin":"mcp-filesystem","args":["--json"],"pass_dirs":true}}"#,
        )
        .unwrap();
        unsafe { std::env::set_var(OPT_IN_ENV, "1") };
        unsafe { std::env::set_var(BIN_DIR_ENV, &dir) };

        let dirs = vec!["C:\\".to_string(), "D:\\data".to_string()];
        let l = resolve("项目文件", "npx", &["-y".into()], &dirs, true);
        assert_eq!(l.backend, Backend::Native);
        assert!(l.command.ends_with(bin_name));
        assert_eq!(l.args, vec!["--json", "C:\\", "D:\\data"]);

        unsafe { std::env::remove_var(BIN_DIR_ENV) };
        unsafe { std::env::remove_var(OPT_IN_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn platform_tag_is_known() {
        assert_ne!(platform_tag(), "unknown");
        assert!(platform_tag().contains('-'));
    }
}
