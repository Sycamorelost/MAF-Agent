//! 报告导出:Markdown / 自包含 HTML / Word(docx) 与另存为路径令牌。
//!
//! Word 走 `fs::docgen` 学术版式;HTML/打印 PDF 的样式由前端 `docStyle` 提供,
//! 三者字号、行距、缩进、三线表一致。

use tauri::Manager;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::memory;
use crate::research::store;

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
    if src_figs.is_dir()
        && let Some(parent) = target.parent()
    {
        let dst = parent.join("figures");
        if std::fs::create_dir_all(&dst).is_ok()
            && let Ok(entries) = std::fs::read_dir(&src_figs)
        {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file()
                    && let Some(name) = p.file_name()
                {
                    let _ = std::fs::copy(&p, dst.join(name));
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

/// 导出报告为 Word(.docx):复用 `fs/docgen` 的学术版式。
///
/// 与工作站「文档生成」走同一套模板(实验/论文/实践/通用),因此研究报告的
/// Word、自包含 HTML、打印 PDF 三种产物在字号、行距、缩进、三线表上保持一致。
/// 相对图片路径以研究目录为基准,报告里的 `figures/xxx` 可直接嵌入。
#[tauri::command]
pub fn research_export_docx(
    id: String,
    token: String,
    overwrite: bool,
    profile: Option<String>,
    tokens: State<'_, ExportPathTokens>,
) -> Result<String, String> {
    export_docx_inner(id, token, overwrite, profile.as_deref(), &tokens)
}

fn export_docx_inner(
    id: String,
    token: String,
    overwrite: bool,
    profile: Option<&str>,
    tokens: &ExportPathTokens,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = ensure_export_path(&tokens.resolve(&token)?.to_string_lossy(), "docx")?;
    let report = store::read_report(&id)?.ok_or("报告不存在或尚未生成")?;
    ensure_writable(&target, overwrite)?;
    let title = report_title(&report);
    let req = crate::fs::docgen::DocRequest {
        body: strip_leading_h1(&report, &title),
        title,
        profile: profile.and_then(crate::fs::docgen::Profile::parse),
        base_dir: store::research_dir(&id),
        ..Default::default()
    };
    let bytes = crate::fs::docgen::render(&req)?;
    std::fs::write(&target, &bytes).map_err(|e| format!("写入 Word 失败: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

/// 从报告 Markdown 的首个一级标题提取文档标题;缺省回退「研究报告」。
fn report_title(report: &str) -> String {
    report
        .lines()
        .find_map(|l| l.trim().strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "研究报告".to_string())
}

/// 正文首个一级标题与文档标题相同时剥离该行:
/// 否则页眉标题与正文 H1 重复出现,且这条 H1 会作为章级条目混进自动目录
/// (真正章节是 ##,会被压在它下一级)。
fn strip_leading_h1(report: &str, title: &str) -> String {
    let trimmed = report.trim_start();
    let Some(rest) = trimmed.strip_prefix("# ") else {
        return report.to_string();
    };
    let line_end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
    if rest[..line_end].trim() == title {
        rest[line_end..].trim_start().to_string()
    } else {
        report.to_string()
    }
}

// ---------- D4:PDF(打印对话框) ----------

/// 触发主窗口的原生打印对话框(用户可选「另存为 PDF」)。
///
/// Tauri 2.12 的 WebView 未暴露 `print_to_pdf` 字节流接口,因此采用原生打印对话框:
/// 渲染层先把「待导出内容」以全屏白底覆盖层铺到主窗口,再调用本命令,
/// 系统打印对话框弹出后用户选择「Microsoft Print to PDF / 另存为 PDF」即可。
/// 这是跨平台(Windows/macOS/Linux)最稳定的 PDF 产出路径,且不依赖额外二进制依赖。
#[tauri::command]
pub fn export_pdf(app: tauri::AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在,无法打印".to_string())?;
    webview.print().map_err(|e| format!("唤起打印失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1 回归:导出接口同样必须拒绝路径遍历 id(id 校验先于令牌解析)。
    #[test]
    fn export_commands_reject_traversal_id() {
        let tokens = ExportPathTokens::default();
        assert!(export_markdown_inner("../x".into(), "t".into(), false, &tokens).is_err());
        assert!(
            export_html_inner("../x".into(), "<html/>".into(), "t".into(), false, &tokens).is_err()
        );
        assert!(export_docx_inner("../x".into(), "t".into(), false, None, &tokens).is_err());
        assert!(super::super::figure::research_figure_data("../x".into(), "a.png".into()).is_err());
    }

    /// N-6 回归:非法令牌一律拒绝,渲染层无法凭空指定路径。
    #[test]
    fn export_commands_reject_unknown_token() {
        let tokens = ExportPathTokens::default();
        let r = export_markdown_inner("valid_id".into(), "no_such_token".into(), false, &tokens);
        assert!(r.is_err());
        assert!(
            export_docx_inner(
                "valid_id".into(),
                "no_such_token".into(),
                false,
                None,
                &tokens
            )
            .is_err()
        );
    }

    /// 正文首个 H1 与标题相同时剥离,避免页眉/正文/目录三处重复;
    /// 不同标题、非首行 H1、无 H1 一律原样保留。
    #[test]
    fn strip_leading_h1_only_removes_matching_first() {
        let md = "# 《X》研究报告\n## TL;DR\n正文";
        assert_eq!(strip_leading_h1(md, "《X》研究报告"), "## TL;DR\n正文");
        // 标题不同:不剥
        let md2 = "# 别的标题\n## TL;DR";
        assert_eq!(strip_leading_h1(md2, "《X》研究报告"), md2);
        // H1 不在首行:不剥
        let md3 = "前言\n# 《X》研究报告\n## TL;DR";
        assert_eq!(strip_leading_h1(md3, "《X》研究报告"), md3);
        // 无 H1:原样
        let md4 = "## TL;DR\n正文";
        assert_eq!(strip_leading_h1(md4, "研究报告"), md4);
        // 前导空白容忍
        let md5 = "\n\n# 《X》研究报告\n\n## TL;DR";
        assert_eq!(strip_leading_h1(md5, "《X》研究报告"), "## TL;DR");
    }
}
