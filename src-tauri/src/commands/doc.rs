//! 文档驻留层的前端直连命令(R4)。
//!
//! `doc_search` 是给模型用的工具,但模型是否调用取决于它对工具的支持与判断。
//! 发送前由前端调 `doc_prefetch` 做一次本地检索,把命中的片段直接拼进用户消息,
//! 长文档分析就不再依赖「工具总开关 / 模型是否愿意调工具」这两个不确定因素。

/// 本地预检索:返回可直接注入用户消息的片段块;无命中或空查询时返回空串。
#[tauri::command]
pub fn doc_prefetch(
    session_id: String,
    query: String,
    doc: Option<String>,
    limit: Option<u32>,
) -> Result<String, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(String::new());
    }
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let out = crate::doc::search(
        &conn,
        &session_id,
        q,
        doc.as_deref(),
        limit.unwrap_or(3).clamp(1, 8) as usize,
    )?;
    if out.hits.is_empty() {
        return Ok(String::new());
    }
    let mut s = String::from(
        "【文档预检索:以下片段由本地检索自动附上,可直接引用】\n未覆盖的细节请再用 doc_search / doc_read 查,不要臆测。",
    );
    for h in &out.hits {
        s.push_str(&format!("\n\n[{} · 第 {} 块", h.doc_name, h.ord + 1));
        if let Some(p) = h.page {
            s.push_str(&format!(" · 第 {p} 页"));
        }
        if !h.section.is_empty() {
            s.push_str(&format!(" · {}", h.section));
        }
        s.push_str(&format!("]\n{}", h.snippet));
    }
    Ok(s)
}
