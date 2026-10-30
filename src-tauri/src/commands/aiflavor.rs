//! 去 AI 味 · AI 生成概率分析的命令层(R3 B2)。
//!
//! 职责:调用本地统计引擎、报告落库/回看/删除、改写记录留档。
//! 改写本身不在这里发起 —— 前端复用 chat_stream + `ai-flavor-rewriter` 智能体,
//! 避免为一次交互再引一套模型调用通道。
//!
//! 存储逻辑全部写成接受 `&Connection` 的纯函数,单测用内存库直接跑迁移。

use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::aiflavor::engine::FlavorReport;
use crate::db;

/// 历史报告摘要(列表页只读这几列,零 JSON 解析)。
#[derive(Debug, Clone, Serialize)]
pub struct AiflavorReportSummary {
    pub id: i64,
    pub created: String,
    pub source_kind: String,
    pub char_count: i64,
    pub score: i64,
    pub level: String,
}

/// 单份历史报告上限:超过即删最旧,避免表无限增长。
const MAX_REPORTS: i64 = 200;

fn now_iso() -> String {
    chrono::Local::now().to_rfc3339()
}

/// 文本指纹(sha256 前 32 位 hex),用于同文重复检测与去重展示。
fn text_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..32].to_string()
}

// ============ 存储(纯函数,可单测) ============

/// 写入一份报告,返回 (id, created)。
fn insert_report(
    conn: &Connection,
    report: &FlavorReport,
    text: &str,
    source_kind: &str,
) -> Result<(i64, String), String> {
    let created = now_iso();
    let report_json = serde_json::to_string(&serde_json::json!({
        "score": report.score,
        "level": report.level,
        "level_code": report.level_code,
        "char_count": report.char_count,
        "degraded": report.degraded,
        "dims": report.dims,
        "sentence_marks": report.sentence_marks,
        "rule_hits": report.rule_hits,
        "stats": report.stats,
        "disclaimer": report.disclaimer,
        "version": report.version,
    }))
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO aiflavor_reports
           (created, source_kind, text_hash, char_count, score, level,
            dims_json, marks_json, stats_json, report_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            created,
            source_kind,
            text_hash(text),
            report.char_count as i64,
            report.score as i64,
            report.level,
            serde_json::to_string(&report.dims).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&report.sentence_marks).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&report.stats).unwrap_or_else(|_| "{}".into()),
            report_json,
        ],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    // 超限清理(只保留最近 MAX_REPORTS 份)
    let _ = conn.execute(
        "DELETE FROM aiflavor_reports WHERE id NOT IN (
           SELECT id FROM aiflavor_reports ORDER BY id DESC LIMIT ?1)",
        params![MAX_REPORTS],
    );
    Ok((id, created))
}

/// 历史列表(新→旧)。
fn list_reports(conn: &Connection, limit: i64) -> Result<Vec<AiflavorReportSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, created, source_kind, char_count, score, level
             FROM aiflavor_reports ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(AiflavorReportSummary {
                id: r.get(0)?,
                created: r.get(1)?,
                source_kind: r.get(2)?,
                char_count: r.get(3)?,
                score: r.get(4)?,
                level: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 回看完整报告(从 report_json 还原,再补 id / created)。
fn get_report(conn: &Connection, id: i64) -> Result<FlavorReport, String> {
    let (created, json): (String, String) = conn
        .query_row(
            "SELECT created, report_json FROM aiflavor_reports WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("报告不存在或读取失败: {e}"))?;
    let mut v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if let Some(obj) = v.as_object_mut() {
        obj.insert("id".into(), serde_json::json!(id));
        obj.insert("created".into(), serde_json::json!(created));
    }
    serde_json::from_value(v).map_err(|e| format!("报告数据损坏: {e}"))
}

/// 删除单份报告(级联删除其改写记录)。
fn delete_report(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM aiflavor_reports WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 记录一次改写(accepted=false 表示仅供对比)。
fn save_rewrite(
    conn: &Connection,
    report_id: i64,
    level: &str,
    diff_json: &str,
    accepted: bool,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO aiflavor_rewrites (report_id, level, diff_json, accepted, created)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![report_id, level, diff_json, accepted as i64, now_iso()],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

// ============ 命令 ============

/// 运行本地检测并落库。
#[tauri::command]
pub async fn aiflavor_analyze(text: String, source_kind: Option<String>) -> Result<FlavorReport, String> {
    if text.trim().is_empty() {
        return Err("文本为空".into());
    }
    let mut report = crate::aiflavor::engine::analyze(&text);
    let kind = source_kind.unwrap_or_else(|| "paste".into());
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let (id, created) = insert_report(&conn, &report, &text, &kind)?;
    report.id = Some(id);
    report.created = Some(created);
    Ok(report)
}

/// 历史报告列表。
#[tauri::command]
pub async fn aiflavor_reports_list(limit: Option<i64>) -> Result<Vec<AiflavorReportSummary>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    list_reports(&conn, limit.unwrap_or(50).clamp(1, 200))
}

/// 回看完整报告。
#[tauri::command]
pub async fn aiflavor_report_get(id: i64) -> Result<FlavorReport, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    get_report(&conn, id)
}

/// 删除单份报告。
#[tauri::command]
pub async fn aiflavor_report_delete(id: i64) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    delete_report(&conn, id)
}

/// 记录一次改写结果。
#[tauri::command]
pub async fn aiflavor_rewrite_save(
    report_id: i64,
    level: String,
    diff_json: String,
    accepted: bool,
) -> Result<i64, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    save_rewrite(&conn, report_id, &level, &diff_json, accepted)
}

/// 导出报告:C4。`format` = md(默认)/ html;HTML 为自包含可打印版(浏览器「打印 → 存为 PDF」)。
/// 落盘到工作区 `exports/`,返回绝对路径供前端展示。
#[tauri::command]
pub async fn aiflavor_export_report(id: i64, format: Option<String>) -> Result<String, String> {
    let report = {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        get_report(&conn, id)?
    };
    let html = format
        .as_deref()
        .unwrap_or("md")
        .eq_ignore_ascii_case("html");
    let (ext, body) = if html {
        ("html", render_html(&report))
    } else {
        ("md", render_md(&report))
    };
    let dir = crate::fs::paths::workspace_root()?.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败: {e}"))?;
    let path = dir.join(format!("aiflavor-report-{id}.{ext}"));
    std::fs::write(&path, body).map_err(|e| format!("写入导出文件失败: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn heat_name(h: u8) -> &'static str {
    match h {
        0 => "正常",
        1 => "轻度",
        2 => "中度",
        _ => "高度",
    }
}

/// 报告 → Markdown(C4 导出)。
fn render_md(r: &FlavorReport) -> String {
    let mut s = String::new();
    s.push_str("# AI 生成概率分析报告\n\n");
    s.push_str(&format!(
        "- 总分:{}/100({})\n- 字数:{}\n- 时间:{}\n- 引擎版本:v{}\n\n",
        r.score,
        r.level,
        r.char_count,
        r.created.clone().unwrap_or_default(),
        r.version
    ));
    if r.degraded {
        s.push_str("> 文本过短(≤100 字),结果仅供参考。\n\n");
    }
    s.push_str("## 六维评分\n\n| 维度 | 原始值 | 评分 | 等级 |\n|---|---|---|---|\n");
    for d in &r.dims {
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            d.name,
            d.raw,
            d.score,
            heat_name(d.level)
        ));
    }
    s.push_str("\n## 规则命中\n\n");
    if r.rule_hits.is_empty() {
        s.push_str("无\n");
    }
    for h in &r.rule_hits {
        s.push_str(&format!(
            "- R{:02} {} ×{}:{}\n",
            h.rule_id,
            h.name,
            h.count,
            h.example.replace('|', "／").replace('\n', " ")
        ));
    }
    s.push_str("\n## 句子热力(前 40 句)\n\n| # | 热力 | 分数 | 原文 |\n|---|---|---|---|\n");
    for m in r.sentence_marks.iter().take(40) {
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            m.index + 1,
            heat_name(m.heat),
            m.score,
            m.text.replace('|', "／").replace('\n', " ")
        ));
    }
    s.push_str(&format!("\n---\n\n{}\n", r.disclaimer));
    s
}

/// 报告 → 自包含 HTML(内嵌样式,可直接打印为 PDF)。
fn render_html(r: &FlavorReport) -> String {
    let mut rows = String::new();
    for d in &r.dims {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td><b>{}</b></td><td>{}</td></tr>",
            esc(&d.name),
            esc(&d.raw),
            d.score,
            heat_name(d.level)
        ));
    }
    let mut rules = String::new();
    for h in &r.rule_hits {
        rules.push_str(&format!(
            "<li>R{:02} {} ×{} — {}</li>",
            h.rule_id,
            esc(&h.name),
            h.count,
            esc(&h.example)
        ));
    }
    let mut marks = String::new();
    for m in r.sentence_marks.iter().take(60) {
        marks.push_str(&format!(
            "<tr class=\"h{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            m.heat,
            m.index + 1,
            heat_name(m.heat),
            m.score,
            esc(&m.text)
        ));
    }
    format!(
        r#"<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<title>AI 生成概率分析报告</title>
<style>
body{{font-family:-apple-system,"Segoe UI","Microsoft YaHei",sans-serif;max-width:860px;margin:40px auto;padding:0 20px;color:#222;line-height:1.7}}
h1{{font-size:24px}} h2{{font-size:17px;margin-top:28px;border-left:4px solid #4a7;padding-left:8px}}
table{{border-collapse:collapse;width:100%;font-size:13px}} td,th{{border:1px solid #ddd;padding:6px 8px;text-align:left}}
th{{background:#f6f8fa}} .score{{font-size:40px;font-weight:700}}
tr.h1 td{{background:rgba(230,126,34,.12)}} tr.h2 td{{background:rgba(231,76,60,.16)}}
tr.h3 td{{background:rgba(192,57,43,.22)}} .note{{color:#666;font-size:12px;margin-top:32px;border-top:1px solid #eee;padding-top:12px}}
@media print{{body{{margin:0}}}}
</style>
<h1>AI 生成概率分析报告</h1>
<p class="score">{}<span style="font-size:16px;font-weight:400"> /100 · {}</span></p>
<p>字数:{} · 时间:{} · 引擎版本:v{}</p>
{}
<h2>六维评分</h2><table><tr><th>维度</th><th>原始值</th><th>评分</th><th>等级</th></tr>{}</table>
<h2>规则命中</h2><ul>{}</ul>
<h2>句子热力(前 60 句)</h2><table><tr><th>#</th><th>热力</th><th>分数</th><th>原文</th></tr>{}</table>
<p class="note">{}</p>
</html>"#,
        r.score,
        esc(&r.level),
        r.char_count,
        esc(&r.created.clone().unwrap_or_default()),
        r.version,
        if r.degraded {
            "<p><b>文本过短(≤100 字),结果仅供参考。</b></p>"
        } else {
            ""
        },
        rows,
        rules,
        marks,
        esc(&r.disclaimer)
    )
}

/// HTML 转义(报告文本可能含用户输入)。
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aiflavor::engine;

    /// 建内存库并跑迁移(与生产同一套 DDL)。
    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn analyze_insert_and_read_back() {
        let conn = mem_db();
        let r = engine::analyze("综上所述，赋能业务，形成闭环，找到抓手，持续优化。");
        let (id, created) = insert_report(&conn, &r, "原文", "paste").unwrap();
        assert!(id > 0);
        assert!(!created.is_empty());

        let back = get_report(&conn, id).unwrap();
        assert_eq!(back.id, Some(id));
        assert_eq!(back.score, r.score);
        assert_eq!(back.dims.len(), 9);
        assert_eq!(back.sentence_marks.len(), r.sentence_marks.len());
        assert!(back.disclaimer.contains("61.2"));
    }

    #[test]
    fn list_is_newest_first_and_respects_limit() {
        let conn = mem_db();
        for i in 0..5 {
            let r = engine::analyze(&format!("第{i}段测试文本，综上所述。"));
            insert_report(&conn, &r, "t", "paste").unwrap();
        }
        let all = list_reports(&conn, 50).unwrap();
        assert_eq!(all.len(), 5);
        assert!(all[0].id > all[4].id, "应新→旧排序");
        let two = list_reports(&conn, 2).unwrap();
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn delete_removes_report_and_rewrites() {
        let conn = mem_db();
        let r = engine::analyze("测试文本，首先，其次，再次。");
        let (id, _) = insert_report(&conn, &r, "t", "paste").unwrap();
        let rid = save_rewrite(&conn, id, "light", "{\"output\":\"x\"}", true).unwrap();
        assert!(rid > 0);
        delete_report(&conn, id).unwrap();
        assert!(get_report(&conn, id).is_err());
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM aiflavor_rewrites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "改写记录应随报告级联删除");
    }

    #[test]
    fn text_hash_is_stable_and_truncated() {
        let h1 = text_hash("同样的文本");
        let h2 = text_hash("同样的文本");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
        assert_ne!(h1, text_hash("不同的文本"));
    }

    #[test]
    fn missing_report_reports_error() {
        let conn = mem_db();
        assert!(get_report(&conn, 9999).is_err());
        assert!(delete_report(&conn, 9999).is_ok(), "删除不存在的行不应报错");
    }
}
