//! 本地文档转 PDF(文件能力增强)。
//!
//! 复用系统已安装的 **LibreOffice**(免费、离线、无需 API Key)做高保真转换:
//!   soffice --headless --convert-to pdf --outdir <dir> <file>
//! 支持 docx / xlsx / pptx / odt / html 等;WPS/Office 不做 COM 自动化,避免平台差异。
//!
//! 未安装时返回可读错误(不阻塞对话),由模型建议用户安装或改用内置导出。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 允许转换的输入扩展名(其余直接拒绝,避免误转二进制)。
const SUPPORTED: [&str; 14] = [
    "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp", "html",
    "htm", "txt",
];

fn office_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["soffice.exe", "libreoffice.exe"]
    } else {
        // 发行版包 / snap / flatpak 的三种可执行文件名
        &["soffice", "libreoffice", "org.libreoffice.LibreOffice"]
    }
}

/// 在候选目录里找 LibreOffice 可执行文件。纯函数,便于单测。
pub fn find_in_dirs(dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        for name in office_names() {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 探测本机 LibreOffice:环境变量 → PATH → 各平台常见安装目录。
pub fn find_office() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("METAAGENT_SOFFICE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
            std::env::var_os("LOCALAPPDATA"),
        ]
        .into_iter()
        .flatten()
        {
            dirs.push(PathBuf::from(base).join("LibreOffice").join("program"));
        }
    } else {
        dirs.push(PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        // Linux:snap 与 flatpak 常不在 PATH(尤其是从桌面图标启动时)
        dirs.push(PathBuf::from("/snap/bin"));
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/bin"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/flatpak/exports/bin"));
        }
    }
    find_in_dirs(&dirs)
}

/// 本地路径 → file:// URL(LibreOffice 的 `-env:UserInstallation` 需要 URL 形式)。
fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

/// 输入文件对应的 PDF 输出路径(<dir>/<stem>.pdf)。
pub fn pdf_output_for(input: &Path, out_dir: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    out_dir.join(format!("{stem}.pdf"))
}

/// 目标 PDF 路径:同名已存在时追加时间戳(同秒重复再追加序号),不覆盖用户文件。
fn pick_target(input: &Path, out_dir: &Path) -> PathBuf {
    let first = pdf_output_for(input, out_dir);
    if !first.exists() {
        return first;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}-{stamp}.pdf")
        } else {
            format!("{stem}-{stamp}-{n}.pdf")
        };
        let candidate = out_dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// 转换为 PDF;返回输出路径。已有同名 PDF 时追加时间戳,不覆盖用户文件。
///
/// 实际输出先落到同目录的临时子目录:LibreOffice 只会按 `<stem>.pdf` 命名,
/// 直接在目标目录转换会覆盖已有同名文件,也无法得到带时间戳的新名字。
pub fn convert_to_pdf(input: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    if !input.is_file() {
        return Err(format!("文件不存在:{}", input.display()));
    }
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !SUPPORTED.contains(&ext.as_str()) {
        return Err(format!("不支持转换为 PDF 的文件类型:{ext}"));
    }
    let office =
        find_office().ok_or_else(|| "未检测到 LibreOffice(免费离线,无需 API)。请安装后重试,或使用内置 Markdown/表格导出。".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("创建输出目录失败:{e}"))?;

    let target = pick_target(input, out_dir);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp_out = out_dir.join(format!(".maf-pdf-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&tmp_out).map_err(|e| format!("创建临时目录失败:{e}"))?;
    // 独立 user profile:避免用户已打开 LibreOffice 时的实例锁导致无头转换失败
    let profile = std::env::temp_dir().join(format!("maf-lo-{}-{nonce}", std::process::id()));

    let mut cmd = Command::new(&office);
    cmd.arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .arg("--headless")
        .arg("--norestore")
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(&tmp_out)
        .arg(input);
    // Windows:无控制台窗口,避免转换时闪出黑窗
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("调用 LibreOffice 失败:{e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let msg = stderr.lines().next().unwrap_or("未知错误").to_string();
        let _ = std::fs::remove_dir_all(&tmp_out);
        let _ = std::fs::remove_dir_all(&profile);
        return Err(format!("LibreOffice 转换失败:{msg}"));
    }

    let produced = pdf_output_for(input, &tmp_out);
    let produced = if produced.is_file() {
        produced
    } else {
        // 少数格式的输出名与输入 stem 不完全一致:取临时目录里的第一个 PDF
        std::fs::read_dir(&tmp_out)
            .ok()
            .and_then(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .find(|p| {
                        p.extension()
                            .map(|e| e.eq_ignore_ascii_case("pdf"))
                            .unwrap_or(false)
                    })
            })
            .ok_or_else(|| "转换完成但未找到 PDF 输出".to_string())?
    };
    let moved = std::fs::rename(&produced, &target).or_else(|_| {
        std::fs::copy(&produced, &target)
            .map(|_| ())
            .and_then(|_| std::fs::remove_file(&produced))
    });
    let _ = std::fs::remove_dir_all(&tmp_out);
    let _ = std::fs::remove_dir_all(&profile);
    moved.map_err(|e| format!("保存 PDF 失败:{e}"))?;
    Ok(target)
}

/// 供 IPC / 工具使用:工作区相对路径 → 绝对路径 → 同目录 PDF。
pub fn convert_in_workspace(
    session_id: Option<&str>,
    raw_path: &str,
) -> Result<PathBuf, String> {
    let root = crate::fs::paths::resolve_workspace_root(session_id)?;
    let input = crate::fs::paths::resolve(&root, raw_path, true)?;
    let out_dir = input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.clone());
    convert_to_pdf(&input, &out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maf-convert-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_soffice_in_candidate_dirs() {
        let dir = tmp("find");
        let exe = if cfg!(windows) { "soffice.exe" } else { "soffice" };
        std::fs::write(dir.join(exe), b"#!/bin/sh\n").unwrap();
        let found = find_in_dirs(&[dir.clone()]).expect("应找到 soffice");
        assert_eq!(found.file_name().unwrap().to_string_lossy(), exe);
    }

    #[test]
    fn ignores_unrelated_binaries() {
        let dir = tmp("none");
        std::fs::write(dir.join("not-office"), b"x").unwrap();
        assert!(find_in_dirs(&[dir]).is_none());
    }

    #[test]
    fn output_path_uses_input_stem() {
        let out = pdf_output_for(Path::new("C:/work/report.docx"), Path::new("C:/work/out"));
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "report.pdf");
    }

    #[test]
    fn missing_input_returns_readable_error() {
        let dir = tmp("missing");
        let err = convert_to_pdf(&dir.join("nope.docx"), &dir).unwrap_err();
        assert!(err.contains("不存在"), "实际:{err}");
    }

    #[test]
    fn file_url_handles_unix_and_windows_paths() {
        assert_eq!(file_url(Path::new("/tmp/x")), "file:///tmp/x");
        assert_eq!(file_url(Path::new(r"C:\Users\a")), "file:///C:/Users/a");
    }

    #[test]
    fn pick_target_never_overwrites_existing_pdf() {
        let dir = tmp("target");
        let input = dir.join("report.docx");
        std::fs::write(&input, b"x").unwrap();
        let first = pick_target(&input, &dir);
        assert_eq!(first.file_name().unwrap().to_string_lossy(), "report.pdf");
        std::fs::write(dir.join("report.pdf"), b"old").unwrap();
        let next = pick_target(&input, &dir);
        let name = next.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("report-"), "实际:{name}");
        assert!(name.ends_with(".pdf"), "实际:{name}");
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let dir = tmp("ext");
        let f = dir.join("a.txt");
        std::fs::write(&f, "hi").unwrap();
        // .txt 在白名单内,改用真正不支持的扩展名
        let f = dir.join("a.exe");
        std::fs::write(&f, "hi").unwrap();
        let err = convert_to_pdf(&f, &dir).unwrap_err();
        assert!(err.contains("不支持"), "实际:{err}");
    }
}
