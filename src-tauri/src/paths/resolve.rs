//! 路径规范化与数据目录候选解析(纯逻辑,不涉及迁移)。

use super::Bootstrap;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tauri::Manager;

/// `app.exe` 同目录。
pub(super) fn exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 尽力规范化路径:存在则 `canonicalize`;不存在则规范化最近的已存在祖先再拼回剩余分量。
///
/// 必须统一两种结果:Windows 下 `canonicalize` 会给存在的路径加 `\\?\` 前缀,而
/// 不存在的路径拿不到前缀,直接比较会让「目录互相包含」的判断失效,进而把目录
/// 复制进自己内部造成无限递归。
pub(super) fn canon(p: &Path) -> PathBuf {
    if let Ok(c) = fs::canonicalize(p) {
        return c;
    }
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    while let Some(name) = cur.file_name() {
        rest.push(name.to_os_string());
        let Some(parent) = cur.parent().map(Path::to_path_buf) else {
            break;
        };
        if let Ok(c) = fs::canonicalize(&parent) {
            let mut out = c;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        if parent.as_os_str().is_empty() {
            break;
        }
        cur = parent;
    }
    p.to_path_buf()
}

/// 去尾部分隔符的小写字符串(Windows 路径大小写不敏感)。
fn key(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/// 去掉 Windows `\\?\` 扩展前缀(UNC 形式还原为 `\\server\share`)。
///
/// `canonicalize` 会给存在的路径加 `\\?\` 前缀;该前缀只对 Win32 长路径 API 有意义,
/// 一旦写进引导配置或被拼进工具参数/附件路径,SQLite、外部脚本与用户看到的都是带前缀的怪路径,
/// 甚至被模型误当作 UNC 网络路径。配置与运行时解析统一使用普通形式。
pub(super) fn plain_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

/// 是否同一路径。
pub(super) fn same_path(a: &Path, b: &Path) -> bool {
    key(&canon(a)) == key(&canon(b))
}

/// 探测目录是否可写(写探针文件再删除)。
pub(super) fn is_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(".maf-write-probe-{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 引导配置目录:exe 旁已有 config.json → 便携模式(exe 旁);否则 Tauri app_config_dir。
/// 旧行为是「exe 可写就用 exe 旁」,release 首启会把数据写进安装目录(F3);现改为显式 opt-in:
/// 想要便携模式,手工(或安装器)在 exe 旁放一个 config.json 即可。
pub(super) fn pick_config_dir(app: &tauri::AppHandle) -> PathBuf {
    let exe = exe_dir();
    if exe.join("config.json").is_file() {
        return exe;
    }
    app.path().app_config_dir().unwrap_or_else(|_| {
        env::var_os("APPDATA")
            .map(|v| PathBuf::from(v).join("com.metaagent.float"))
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

/// 历史数据目录(老版本编译期固化的位置),仅用于升级兼容与开发期默认。
fn legacy_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}

/// 按优先级解析数据目录(不依赖 AppHandle)。
pub(super) fn resolve_data_dir(boot: &Bootstrap) -> PathBuf {
    plain_path(&resolve_data_dir_raw(boot))
}

fn resolve_data_dir_raw(boot: &Bootstrap) -> PathBuf {
    // 1. 环境变量覆盖
    if let Some(v) = env::var_os("MYAGENT_DATA_DIR")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    // 单测:固定用仓库 data/(与历史行为一致,避免误写用户目录)
    if cfg!(test) {
        return legacy_data_dir();
    }
    // 2. 引导配置
    if let Some(d) = boot.data_dir.as_deref()
        && !d.is_empty()
    {
        return PathBuf::from(d);
    }
    // 3. 历史兼容(老版本升级)
    let legacy = legacy_data_dir();
    if legacy.join("metaagent.db").exists() {
        return legacy;
    }
    // 4. 开发期
    if cfg!(debug_assertions) {
        return legacy;
    }
    // 5. 待定目录
    super::config_dir().join("data")
}
