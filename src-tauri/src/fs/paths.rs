//! 文件路径解析 —— 不设沙箱:绝对路径直达任意位置,相对路径以工作区为基准。
//!
//! 规则:
//! 1. 绝对路径(含盘符 / UNC)直接使用,可访问整台电脑上的文件;
//! 2. 相对路径 join 到工作区根,允许 `..` 自由跳转;
//! 3. 读取时 canonicalize(解析符号链接与 `..`)。
//!
//! 不设任何文件名黑名单,位置与文件类型均不受限。

use std::path::{Path, PathBuf};

/// 工作区根目录:环境变量 `MYAGENT_WORKSPACE` 优先,其次用户在设置里配的 `workspace_root`,
/// 否则 `<数据目录>/workspace`。这是「相对路径」的默认基准目录(绝对路径不受其限制)。
pub fn workspace_root() -> Result<PathBuf, String> {
    let root = match std::env::var("MYAGENT_WORKSPACE") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
        _ => {
            let s = crate::settings::load();
            match s.workspace_root.as_deref() {
                Some(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
                _ => crate::paths::data_dir().join("workspace"),
            }
        }
    };
    std::fs::create_dir_all(&root).map_err(|e| format!("创建工作区失败: {e}"))?;
    root.canonicalize()
        .map_err(|e| format!("工作区不可访问 `{}`: {e}", root.display()))
}

/// 智能工作区根 —— 如果 session 绑定了项目且项目 path 有效,返回项目路径;
/// 否则回退全局 `workspace_root()`。MCP tool dispatch 时用这个替换硬编码 workspace_root。
pub fn resolve_workspace_root(session_id: Option<&str>) -> Result<PathBuf, String> {
    if let Some(sid) = session_id {
        if let Ok(Some(session)) = crate::db::sessions::get(sid) {
            if let Some(ref pid) = session.project_id {
                if let Ok(Some(proj)) = crate::db::projects::get(pid) {
                    let p = PathBuf::from(&proj.path);
                    if p.is_dir() {
                        // Windows UNC 清理 + canonicalize
                        let clean = proj
                            .path
                            .strip_prefix(r"\\?\")
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| proj.path.clone());
                        let canonical = PathBuf::from(&clean).canonicalize();
                        match canonical {
                            Ok(c) => return Ok(c),
                            Err(_) => return Ok(PathBuf::from(&clean)),
                        }
                    }
                }
            }
        }
    }
    workspace_root()
}

/// 安全检查:目标路径必须在允许根目录下(canonicalize 后 starts_with 校验)。
/// 用于项目绑定的写操作兜底,防止 Agent 通过绝对路径或 `..` 跳出项目目录。
///
/// `target` 可以是相对路径(会先 join 到 root)或绝对路径(直接校验)。
/// 返回规范化后的目标路径;越界时返回错误。
pub fn ensure_within(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let abs_root = root
        .canonicalize()
        .map_err(|e| format!("项目目录无效 `{}`: {e}", root.display()))?;
    // 目标尚不存在(写新文件)时 canonicalize 会失败,退化到 canonicalize_for
    let abs_target = if target.is_absolute() {
        canonicalize_for(target)
    } else {
        let joined = abs_root.join(target);
        canonicalize_for(&joined)
    };
    // Windows UNC 前缀统一后比较
    let abs_root_str = display(&abs_root);
    let abs_target_str = display(&abs_target);
    if !abs_target_str.starts_with(&abs_root_str) {
        return Err(format!(
            "路径安全:目标 `{abs_target_str}` 不在项目工作区 `{abs_root_str}` 下"
        ));
    }
    Ok(abs_target)
}

/// 路径规范化:存在则 canonicalize,不存在则规范化父目录再拼文件名。
fn canonicalize_for(path: &Path) -> PathBuf {
    if let Ok(c) = path.canonicalize() {
        return c;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => {
            let pc = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            pc.join(name)
        }
        _ => path.to_path_buf(),
    }
}

/// 去掉 canonicalize 产生的 `\\?\` 前缀,便于跨平台比较与回显。
fn display(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// 把路径解析为绝对路径:
/// - 绝对路径(盘符 / UNC / 前导 `\`)直接使用,可指向本机任意位置;
/// - 相对路径 join 到工作区根,`..` 不做限制。
///
/// `must_exist=true` 时要求目标已存在(读取);false 用于写入(父目录可稍后创建)。
pub fn resolve(root: &Path, rel: &str, must_exist: bool) -> Result<PathBuf, String> {
    let rel = rel.trim();
    if rel.is_empty() {
        return Err("路径不能为空".into());
    }
    if rel.contains('\0') || rel.contains('\r') || rel.contains('\n') {
        return Err("路径包含非法字符(NUL/CR/LF)".into());
    }
    let raw = Path::new(rel);
    let absolute = raw.is_absolute() || rel.starts_with('\\') || rel.chars().nth(1) == Some(':');
    let target = if absolute {
        raw.to_path_buf()
    } else {
        // Windows 上 canonicalize 会带 `\\?\` 前缀,统一用规范化后的根拼接
        let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        root_canon.join(raw)
    };
    if must_exist {
        let canonical = target
            .canonicalize()
            .map_err(|e| format!("路径不存在或无法访问 `{rel}`: {e}"))?;
        return Ok(canonical);
    }
    Ok(canonical_or_parent(&target))
}

/// 规范化路径;目标尚不存在(写新文件)时规范化其父目录再拼接文件名。
fn canonical_or_parent(path: &Path) -> PathBuf {
    if let Ok(c) = path.canonicalize() {
        return c;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => {
            let pc = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            pc.join(name)
        }
        _ => path.to_path_buf(),
    }
}

/// 绝对路径 → 工作区相对路径(正斜杠),用于备份与展示。
pub fn rel_of(root: &Path, path: &Path) -> Result<String, String> {
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path_canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let rel = path_canon
        .strip_prefix(&root_canon)
        .or_else(|_| path.strip_prefix(root))
        .map_err(|_| format!("`{}` 不在工作区内", path.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        std::env::current_dir().unwrap()
    }

    #[test]
    fn accepts_absolute_and_parent_paths() {
        let r = root();
        // 绝对路径:全盘可访问(用临时文件验证,不依赖系统自带文件是否存在)
        let outside = std::env::temp_dir().join("maf-paths-abs.txt");
        std::fs::write(&outside, "x").unwrap();
        let p = resolve(&r, outside.to_str().unwrap(), true).expect("绝对路径必须可读");
        assert!(p.ends_with("maf-paths-abs.txt"), "实际:{}", p.display());
        // 写入尚不存在的绝对路径同样放行
        let new_file = std::env::temp_dir().join("maf-paths-new.txt");
        assert!(resolve(&r, new_file.to_str().unwrap(), false).is_ok());
        // 相对路径 `..` 不再拦截;空路径仍拒绝
        assert!(resolve(&r, "../whatever.txt", false).is_ok());
        assert!(resolve(&r, "", false).is_err());
        std::fs::remove_file(&outside).ok();
    }

    #[test]
    fn accepts_workspace_relative_path() {
        let r = root();
        let p = resolve(&r, "src/lib.rs", true).expect("应能解析仓库内文件");
        // 返回值是 canonicalize 后的形式(Windows 带 \\?\ 前缀),与规范化根比较
        assert!(p.starts_with(r.canonicalize().unwrap()));
    }
}
