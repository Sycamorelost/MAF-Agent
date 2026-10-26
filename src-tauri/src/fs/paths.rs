//! 工作区路径解析 —— 文件工具只接受工作区内的相对路径。
//!
//! 规则:
//! 1. 拒绝绝对路径 / 盘符 / UNC,避免"指向任意位置";
//! 2. 拒绝 `..`,保证 join 后仍在工作区内;
//! 3. 读取时 canonicalize 并二次校验(防符号链接逃逸);
//! 4. 命中敏感文件名(`.env` / `id_rsa` / `*.pem` …)一律拒绝。

use std::path::{Component, Path, PathBuf};

use crate::mcp::safety::is_sensitive;

/// 工作区根目录:环境变量 `MAF_WORKSPACE` 优先,否则 `<数据目录>/workspace`。
pub fn workspace_root() -> Result<PathBuf, String> {
    let root = match std::env::var("MAF_WORKSPACE") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => crate::paths::data_dir().join("workspace"),
    };
    std::fs::create_dir_all(&root).map_err(|e| format!("创建工作区失败: {e}"))?;
    root.canonicalize()
        .map_err(|e| format!("工作区不可访问 `{}`: {e}", root.display()))
}

/// 把相对路径解析为工作区内的绝对路径。
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
    if raw.is_absolute() || rel.starts_with('\\') || rel.chars().nth(1) == Some(':') {
        return Err("只允许工作区内的相对路径,不能使用绝对路径".into());
    }
    for c in raw.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("路径不能包含 `..` 或根前缀".into()),
        }
    }
    if is_sensitive(raw) {
        return Err(format!("`{rel}` 命中敏感文件黑名单,已拒绝"));
    }
    // Windows 上 canonicalize 会带 `\\?\` 前缀,统一用规范化后的根拼接与比较
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let joined = root_canon.join(raw);
    if must_exist {
        let canonical = joined
            .canonicalize()
            .map_err(|e| format!("路径不存在或无法访问 `{rel}`: {e}"))?;
        if !canonical.starts_with(&root_canon) {
            return Err(format!("`{rel}` 超出工作区,已拒绝"));
        }
        if is_sensitive(&canonical) {
            return Err(format!("`{rel}` 命中敏感文件黑名单,已拒绝"));
        }
        return Ok(canonical);
    }
    Ok(joined)
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
    fn rejects_absolute_and_parent_paths() {
        let r = root();
        assert!(resolve(&r, "C:\\Windows\\win.ini", true).is_err());
        assert!(resolve(&r, "../secret.txt", false).is_err());
        assert!(resolve(&r, "/etc/passwd", true).is_err());
        assert!(resolve(&r, "", false).is_err());
    }

    #[test]
    fn rejects_sensitive_names() {
        let r = root();
        assert!(resolve(&r, ".env", false).is_err());
        assert!(resolve(&r, "keys/id_rsa", false).is_err());
    }

    #[test]
    fn accepts_workspace_relative_path() {
        let r = root();
        let p = resolve(&r, "src/lib.rs", true).expect("应能解析仓库内文件");
        // 返回值是 canonicalize 后的形式(Windows 带 \\?\ 前缀),与规范化根比较
        assert!(p.starts_with(r.canonicalize().unwrap()));
    }
}
