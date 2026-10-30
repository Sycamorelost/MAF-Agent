#![allow(dead_code)]

//! 导出路径令牌与路径规范(自 commands/research/export.rs 迁出,行为不变)。

/// 目标已存在但未获覆盖确认时返回的错误码;前端据此弹二次确认后带 overwrite=true 重试。
pub const EXPORT_TARGET_EXISTS: &str = "TARGET_EXISTS";

/// 导出路径令牌表(N-6):token → 用户经原生「另存为」对话框选定的路径。
/// 导出命令只认令牌,渲染层不能凭空指定任意路径/任意覆盖——彻底收敛 H-2 残余。
#[derive(Default)]
pub struct ExportPathTokens(
    std::sync::Mutex<std::collections::HashMap<String, std::path::PathBuf>>,
);

impl ExportPathTokens {
    /// 登记对话框选定路径,返回一次性语义的令牌(表有界,满 64 清空重建)。
    pub(crate) fn issue(&self, path: std::path::PathBuf) -> String {
        use rand::RngCore;
        let mut b = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut b);
        let token = b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        if let Ok(mut m) = self.0.lock() {
            if m.len() >= 64 {
                m.clear();
            }
            m.insert(token.clone(), path);
        }
        token
    }

    /// 按令牌取回路径(不消费:用户可能对同一目标重复导出)。
    pub(crate) fn resolve(&self, token: &str) -> Result<std::path::PathBuf, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .get(token)
            .cloned()
            .ok_or_else(|| "导出路径令牌无效或已过期,请重新选择保存位置".to_string())
    }
}

/// 另存为对话框结果(H-2/N-6):附带选择时目标是否已存在 + 后端签发的路径令牌。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathPick {
    pub path: String,
    pub existed: bool,
    pub token: String,
}

/// 导出路径规范(H-2):必须绝对路径、扩展名与白名单一致、父目录已存在(不替调用方创建任意目录)。
/// 会话导出(commands/memory/crud.rs)复用同一套校验。
pub fn ensure_export_path(raw: &str, allowed_ext: &str) -> Result<std::path::PathBuf, String> {
    let target = std::path::PathBuf::from(raw.trim());
    if !target.is_absolute() {
        return Err("导出路径必须为绝对路径".into());
    }
    let want = allowed_ext.trim_start_matches('.');
    let ext_ok = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(want))
        .unwrap_or(false);
    if !ext_ok {
        return Err(format!("导出文件扩展名必须为 .{want}"));
    }
    let parent = target.parent().ok_or("导出路径缺少父目录")?;
    if !parent.is_dir() {
        return Err("导出目录不存在,请通过另存为对话框选择有效位置".into());
    }
    Ok(target)
}

/// 覆盖校验(H-2):目标已是文件且未显式 overwrite 时拒绝(返回 TARGET_EXISTS);
/// 同名目录一律拒绝,避免目录被文件覆盖的怪异状态。会话导出同样复用。
pub fn ensure_writable(target: &std::path::Path, overwrite: bool) -> Result<(), String> {
    match target.metadata() {
        Ok(meta) if meta.is_file() => {
            if overwrite {
                Ok(())
            } else {
                Err(EXPORT_TARGET_EXISTS.into())
            }
        }
        Ok(_) => Err("导出目标已存在且是目录,无法写入".into()),
        Err(_) => Ok(()),
    }
}
