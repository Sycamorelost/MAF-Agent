//! 报告导出:Markdown / 自包含 HTML 与另存为路径令牌。

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::memory;
use crate::research::store;

// ---------- 报告导出(v1.9.0:仅 Markdown 与自包含 HTML,不做 PDF/DOCX/PPTX) ----------

/// 目标已存在但未获覆盖确认时返回的错误码;前端据此弹二次确认后带 overwrite=true 重试。
pub const EXPORT_TARGET_EXISTS: &str = "TARGET_EXISTS";

/// 导出路径令牌表(N-6):token → 用户经原生「另存为」对话框选定的路径。
/// 导出命令只认令牌,渲染层不能凭空指定任意路径/任意覆盖——彻底收敛 H-2 残余。
#[derive(Default)]
pub struct ExportPathTokens(std::sync::Mutex<std::collections::HashMap<String, std::path::PathBuf>>);

impl ExportPathTokens {
    /// 登记对话框选定路径,返回一次性语义的令牌(表有界,满 64 清空重建)。
    fn issue(&self, path: std::path::PathBuf) -> String {
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
fn ensure_export_path(raw: &str, allowed_ext: &str) -> Result<std::path::PathBuf, String> {
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
/// 同名目录一律拒绝,避免目录被文件覆盖的怪异状态。
fn ensure_writable(target: &std::path::Path, overwrite: bool) -> Result<(), String> {
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

/// 弹出原生「另存为」对话框(报告导出入口);用户取消返回 None。
/// 与 pick_data_dir 同款实现:阻塞调用放入 blocking 线程池,避免卡住运行时。
/// 选定路径登记为令牌(N-6),后续导出命令只认令牌。
#[tauri::command]
pub async fn pick_save_path(
    app: tauri::AppHandle,
    tokens: State<'_, ExportPathTokens>,
    title: String,
    default_name: String,
    filter_name: String,
    filter_ext: String,
) -> Result<Option<SavePathPick>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .set_file_name(default_name)
            .add_filter(filter_name, &[filter_ext.as_str()])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    // 此时应用尚未写入:存在即代表覆盖了用户的旧文件,系统对话框已征得确认
    let existed = picked.is_file();
    let token = tokens.issue(picked.clone());
    Ok(Some(SavePathPick {
        path: picked.display().to_string(),
        existed,
        token,
    }))
}

/// 导出报告为 Markdown:写入用户选定路径,同目录创建 figures/ 资源夹并复制图示资产。
#[tauri::command]
pub fn research_export_markdown(
    id: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, ExportPathTokens>,
) -> Result<String, String> {
    export_markdown_inner(id, token, overwrite, &tokens)
}

fn export_markdown_inner(
    id: String,
    token: String,
    overwrite: bool,
    tokens: &ExportPathTokens,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    // 令牌解析先于 DB 读取:非法令牌快速失败,也便于无库单测
    let target = ensure_export_path(&tokens.resolve(&token)?.to_string_lossy(), "md")?;
    let report = store::read_report(&id)?.ok_or("报告不存在或尚未生成")?;
    ensure_writable(&target, overwrite)?;
    std::fs::write(&target, &report).map_err(|e| format!("写入 Markdown 失败: {e}"))?;
    // 图示资产:同目录 figures/ 复制(报告内相对引用 figures/xxx 直接可预览)。
    // 父目录已由 ensure_export_path 确认存在,仅在其下建资产夹,不触碰其他路径。
    let src_figs = store::research_dir(&id).join("figures");
    if src_figs.is_dir() {
        if let Some(parent) = target.parent() {
            let dst = parent.join("figures");
            if std::fs::create_dir_all(&dst).is_ok() {
                if let Ok(entries) = std::fs::read_dir(&src_figs) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_file() {
                            if let Some(name) = p.file_name() {
                                let _ = std::fs::copy(&p, dst.join(name));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(target.to_string_lossy().into_owned())
}

/// 导出报告为自包含 HTML:前端序列化已渲染 DOM,后端将 figures/ 引用内嵌为 base64 后写盘。
#[tauri::command]
pub fn research_export_html(
    id: String,
    html: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, ExportPathTokens>,
) -> Result<String, String> {
    export_html_inner(id, html, token, overwrite, &tokens)
}

fn export_html_inner(
    id: String,
    html: String,
    token: String,
    overwrite: bool,
    tokens: &ExportPathTokens,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = ensure_export_path(&tokens.resolve(&token)?.to_string_lossy(), "html")?;
    let figures = store::research_dir(&id).join("figures");
    let html = super::figure::inline_figures(&html, &figures);
    ensure_writable(&target, overwrite)?;
    std::fs::write(&target, html).map_err(|e| format!("写入 HTML 失败: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1 回归:导出接口同样必须拒绝路径遍历 id(id 校验先于令牌解析)。
    #[test]
    fn export_commands_reject_traversal_id() {
        let tokens = ExportPathTokens::default();
        assert!(export_markdown_inner("../x".into(), "t".into(), false, &tokens).is_err());
        assert!(export_html_inner("../x".into(), "<html/>".into(), "t".into(), false, &tokens).is_err());
        assert!(super::super::figure::research_figure_data("../x".into(), "a.png".into()).is_err());
    }

    /// N-6 回归:非法令牌一律拒绝,渲染层无法凭空指定路径。
    #[test]
    fn export_commands_reject_unknown_token() {
        let tokens = ExportPathTokens::default();
        let r = export_markdown_inner("valid_id".into(), "no_such_token".into(), false, &tokens);
        assert!(r.is_err());
    }
}
