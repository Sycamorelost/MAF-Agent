//! 文档驻留层(R3 C1/C2,val.md §2.3 / §8 C1)。
//!
//! 附件在保存时提取的正文不再只塞进消息 JSON:同时按块(目标 1000 字符)入库
//! `doc_chunks`,保留页码与章节路径。会话内可通过 `doc_search` / `doc_read`
//! 内置工具按需回看任意章节 —— 长文档不再只靠一次性注入,也不会因上下文裁剪而「失忆」。
//!
//! 生命周期:块随会话级联删除(表上 `REFERENCES sessions(id) ON DELETE CASCADE`),
//! 不入任何远端;本模块只用本地 SQLite。
//!
//! 编排入口在 [`service`](service):提取入库 / 摘要 / 删除 / 目录迁移都走那里,
//! 本文件只放分块、打分检索等纯算法。

pub mod service;

#[cfg(test)]
mod bench;

use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// 单块目标字符数(不含空白)。
pub const CHUNK_TARGET: usize = 1000;

/// `doc_read` 单次返回的最大字符数(防止模型一次拉爆上下文)。
pub const READ_MAX_CHARS: usize = 12_000;

// ============ 分块 ============

/// 一个文本块:页码(PDF/其它分页来源)与所属章节可为空。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocChunk {
    pub page: Option<u32>,
    pub section: String,
    pub content: String,
}

/// `【第 N 页】` 标记解析(纯文本层 PDF 由 `attachment::extract_pdf` 逐页拼出)。
fn parse_page_marker(line: &str) -> Option<u32> {
    let t = line.trim();
    let rest = t.strip_prefix("【第")?;
    let num = rest.strip_suffix("页】")?.trim();
    num.parse::<u32>().ok()
}

/// 形如章节标题的行(标题行不入正文块,只更新后续块的 section 元数据)。
fn is_heading(line: &str) -> bool {
    let t = line.trim();
    let n = t.chars().count();
    if n == 0 || n > 40 {
        return false;
    }
    if t.starts_with('#') {
        return true;
    }
    if t.starts_with("第") && (t.contains('章') || t.contains('节')) {
        return true;
    }
    const CN: [char; 10] = ['一', '二', '三', '四', '五', '六', '七', '八', '九', '十'];
    let mut it = t.chars();
    matches!(it.next(), Some(c) if CN.contains(&c)) && it.next() == Some('、')
}

fn clean_heading(line: &str) -> String {
    line.trim().trim_start_matches('#').trim().to_string()
}

fn flush(out: &mut Vec<DocChunk>, buf: &mut String, page: Option<u32>, section: &str) {
    let content = buf.trim();
    if !content.is_empty() {
        out.push(DocChunk {
            page,
            section: section.to_string(),
            content: content.to_string(),
        });
    }
    buf.clear();
}

/// 按行切块:遇到页码标记/章节标题即断开,正文累积到 `CHUNK_TARGET` 后换块。
pub fn chunk_text(text: &str) -> Vec<DocChunk> {
    let mut out: Vec<DocChunk> = Vec::new();
    let mut page: Option<u32> = None;
    let mut section = String::new();
    let mut buf = String::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(p) = parse_page_marker(line) {
            flush(&mut out, &mut buf, page, &section);
            page = Some(p);
            section.clear();
            continue;
        }
        if is_heading(line) {
            flush(&mut out, &mut buf, page, &section);
            section = clean_heading(line);
            continue;
        }
        if line.trim().is_empty() {
            if !buf.is_empty() {
                buf.push('\n');
            }
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if buf.chars().filter(|c| !c.is_whitespace()).count() >= CHUNK_TARGET {
            flush(&mut out, &mut buf, page, &section);
        }
    }
    flush(&mut out, &mut buf, page, &section);
    out
}

/// 文档摘要卡:开头正文 + 章节骨架 + 规模,替代「只取前 200 字」。
/// 卡片随每轮对话注入,必须在几百字符内让人/模型判断「这份文档讲什么、去哪找」。
pub fn summary(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let head: String = flat.chars().take(320).collect();
    // 章节骨架:最多 12 个标题,去重、跳过「第 N 页」这类页码标记
    let mut sections: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if !(t.starts_with("## ") || t.starts_with("# ") || t.starts_with("### ")) {
            continue;
        }
        let title = t.trim_start_matches('#').trim();
        if title.is_empty() || title.starts_with("第 ") {
            continue;
        }
        if !sections.iter().any(|s| s == title) {
            sections.push(title.to_string());
        }
        if sections.len() >= 12 {
            break;
        }
    }
    let chars = text.chars().count();
    let mut out = head;
    if chars > 320 {
        out.push('…');
    }
    out.push_str(&format!("\n(全文约 {chars} 字符"));
    if !sections.is_empty() {
        out.push_str(&format!(";章节:{})", sections.join(" / ")));
    } else {
        out.push(')');
    }
    out
}

/// 文档指纹:正文内容的 sha256 前 12 位。
/// 用内容而不是路径,同一文件重复上传(哪怕落盘路径不同)也视为同一文档,
/// 不会重复建块(R14)。
pub fn doc_id_of(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..12].to_string()
}

// ============ 存储 ============

/// 文档概要(给模型看,也给前端做文档卡)。
#[derive(Debug, Clone, Serialize)]
pub struct DocInfo {
    pub doc_id: String,
    pub doc_name: String,
    pub kind: String,
    pub chunks: i64,
    pub chars: i64,
    pub page_from: Option<i64>,
    pub page_to: Option<i64>,
}

/// 检索命中。
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub doc_name: String,
    pub ord: i64,
    pub page: Option<i64>,
    pub section: String,
    pub snippet: String,
    pub score: i64,
}

/// 检索结果(附带扫描块数,供模型判断是否还有更多)。
#[derive(Debug, Clone, Serialize)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub scanned: usize,
}

/// 建/重建一份文档的块索引:先删同 (会话, 文档) 旧块,再整份写入。
/// 返回 (doc_id, 块数);doc_id 由正文内容哈希决定。
pub fn index_document(
    conn: &Connection,
    session_id: &str,
    doc_name: &str,
    kind: &str,
    text: &str,
) -> Result<(String, usize), String> {
    let doc_id = doc_id_of(text);
    let chunks = chunk_text(text);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // 同一会话内:同 doc_id(内容相同)或同 doc_name(同名文件重新上传)都视为同一份文档,
    // 重建索引时先清旧块,避免"同名新版本"叠成两份(R14)。
    // FTS 不随外键级联,必须先按 rowid 清旧索引再删块(V27)。
    tx.execute(
        "DELETE FROM doc_chunks_fts WHERE rowid IN
           (SELECT id FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3))",
        params![session_id, doc_id, doc_name],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3)",
        params![session_id, doc_id, doc_name],
    )
    .map_err(|e| e.to_string())?;
    let created = chrono::Local::now().to_rfc3339();
    for (i, c) in chunks.iter().enumerate() {
        tx.execute(
            "INSERT INTO doc_chunks
               (session_id, doc_id, doc_name, kind, page, section, ord, content, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                doc_id,
                doc_name,
                kind,
                c.page.map(|p| p as i64),
                c.section,
                i as i64,
                c.content,
                created,
            ],
        )
        .map_err(|e| e.to_string())?;
        // 同步入 FTS 索引(rowid = doc_chunks.id;章节标题一并入索引,V27)
        let rowid = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO doc_chunks_fts(rowid, content) VALUES (?1, ?2)",
            params![rowid, format!("{} {}", c.section, c.content)],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok((doc_id, chunks.len()))
}

/// 会话内全部已驻留文档。
pub fn list_docs(conn: &Connection, session_id: &str) -> Result<Vec<DocInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT doc_id, doc_name, kind, COUNT(*), COALESCE(SUM(length(content)),0),
                    MIN(page), MAX(page)
             FROM doc_chunks WHERE session_id=?1
             GROUP BY doc_id, doc_name, kind ORDER BY MIN(id)",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(DocInfo {
                doc_id: r.get(0)?,
                doc_name: r.get(1)?,
                kind: r.get(2)?,
                chunks: r.get(3)?,
                chars: r.get(4)?,
                page_from: r.get(5)?,
                page_to: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 文档骨架(供记忆提炼使用):每份文档的名称 / 规模 / 章节标题,不含正文。
/// 记忆提炼的输入预算只有 8000 字符,放不下正文;给出骨架让提炼器能维护
/// 「文档摘要」节,而不是让模型凭空气写。
pub fn outline(conn: &Connection, session_id: &str) -> Result<String, String> {
    let docs = list_docs(conn, session_id)?;
    if docs.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::new();
    for d in docs {
        let mut stmt = conn
            .prepare(
                "SELECT section, MIN(ord) AS o FROM doc_chunks
                 WHERE session_id=?1 AND doc_name=?2 AND section <> ''
                 GROUP BY section ORDER BY o LIMIT 12",
            )
            .map_err(|e| e.to_string())?;
        let sections: Vec<String> = stmt
            .query_map(params![session_id, d.doc_name], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        out.push_str(&format!(
            "\n- 《{}》({} · {} 块 · 约 {} 字符)",
            d.doc_name,
            kind_label(&d.kind),
            d.chunks,
            d.chars
        ));
        if !sections.is_empty() {
            out.push_str(&format!(";章节:{}", sections.join(" / ")));
        }
    }
    Ok(out)
}

/// 文档类型的展示名(给记忆骨架用)。
fn kind_label(kind: &str) -> &str {
    match kind {
        "pdf" => "PDF",
        "docx" => "Word",
        "xlsx" => "Excel",
        "pptx" => "PPT",
        "epub" => "EPUB",
        "html" => "HTML",
        _ => "文本",
    }
}

/// 检索停用词与剥离逻辑已上移至 `crate::retrieval`(全局搜索与文档检索共用,单一事实源)。
use crate::retrieval::strip_stopwords;

/// CJK 连续片段 → 2 字 / 3 字 n-gram(短片段保留整词)。
/// 中文没有空格分词,按整句做字面匹配几乎不可能命中;n-gram 近似倒排索引。
fn push_cjk_ngrams(run: &[char], out: &mut Vec<String>) {
    match run.len() {
        0 | 1 => {}
        2 => {
            out.push(run.iter().collect());
        }
        3 => {
            out.push(run.iter().collect());
            for w in run.windows(2) {
                out.push(w.iter().collect());
            }
        }
        _ => {
            for w in run.windows(2) {
                out.push(w.iter().collect());
            }
            for w in run.windows(3) {
                out.push(w.iter().collect());
            }
            if run.len() <= 4 {
                out.push(run.iter().collect());
            }
        }
    }
}

/// 把检索词切成有效 token:ASCII 词(≥2 字符)+ 中文 2~3 字 n-gram。
/// 返回去重后的 token,长 token 优先,最多 24 个(控制 SQL 参数与噪声)。
pub fn query_tokens(query: &str) -> Vec<String> {
    let cleaned = strip_stopwords(query);
    let mut out: Vec<String> = Vec::new();
    let mut ascii = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for c in cleaned.chars() {
        let is_cjk = ('\u{4e00}'..='\u{9fff}').contains(&c);
        if c.is_ascii_alphanumeric() {
            if !cjk.is_empty() {
                push_cjk_ngrams(&cjk, &mut out);
                cjk.clear();
            }
            ascii.push(c);
        } else if is_cjk {
            if !ascii.is_empty() {
                if ascii.chars().count() >= 2 {
                    out.push(std::mem::take(&mut ascii));
                } else {
                    ascii.clear();
                }
            }
            cjk.push(c);
        } else {
            if !ascii.is_empty() {
                if ascii.chars().count() >= 2 {
                    out.push(std::mem::take(&mut ascii));
                } else {
                    ascii.clear();
                }
            }
            if !cjk.is_empty() {
                push_cjk_ngrams(&cjk, &mut out);
                cjk.clear();
            }
        }
    }
    if ascii.chars().count() >= 2 {
        out.push(ascii);
    }
    if !cjk.is_empty() {
        push_cjk_ngrams(&cjk, &mut out);
    }

    // 长 token 更具体:按「长度降序 + 字面升序」排序,保证相同 token 相邻,
    // 再去重并保留前 24 个(控制 SQL 参数与噪声)。
    out.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    out.dedup();
    out.truncate(24);
    out
}

/// 片段截取:命中位置前后各 60 字符。
fn snippet_of(content: &str, token: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = flat.to_lowercase();
    let Some(pos) = lower.find(token) else {
        return flat.chars().take(120).collect();
    };
    let chars: Vec<char> = flat.chars().collect();
    let byte_pos = lower[..pos].chars().count();
    let start = byte_pos.saturating_sub(60);
    let end = (byte_pos + token.chars().count() + 60).min(chars.len());
    let head = if start > 0 { "…" } else { "" };
    let tail = if end < chars.len() { "…" } else { "" };
    format!(
        "{head}{}{tail}",
        chars[start..end].iter().collect::<String>()
    )
}

/// 覆盖率打分候选块(路 B:LIKE 粗筛 + 「命中覆盖率 × 词长权重」打分)。
struct Cand {
    id: i64,
    doc_name: String,
    ord: i64,
    page: Option<i64>,
    section: String,
    content: String,
    cov_score: i64,
    /// 命中的最长 token(用于截取片段)。
    best: String,
}

/// 路 B:覆盖率候选。SQL 层按 token 粗筛,内存按覆盖率打分。
fn coverage_candidates(
    conn: &Connection,
    session_id: &str,
    tokens: &[String],
    doc_hint: Option<&str>,
) -> Result<Vec<Cand>, String> {
    let mut sql = String::from(
        "SELECT id, doc_name, ord, page, section, content FROM doc_chunks WHERE session_id=?",
    );
    let mut values: Vec<rusqlite::types::Value> =
        vec![rusqlite::types::Value::Text(session_id.to_string())];
    if let Some(h) = doc_hint {
        sql.push_str(" AND (doc_id=? OR doc_name LIKE '%'||?||'%')");
        values.push(rusqlite::types::Value::Text(h.to_string()));
        values.push(rusqlite::types::Value::Text(h.to_string()));
    }
    // 块级粗筛:正文或章节标题命中任一 token 即进入打分阶段(比全表拉取快一个量级,R15)。
    sql.push_str(" AND (");
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(content || ' ' || section) LIKE ?");
        values.push(rusqlite::types::Value::Text(format!("%{t}%")));
    }
    sql.push(')');
    sql.push_str(" ORDER BY ord");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut cands: Vec<Cand> = Vec::new();
    for (id, doc_name, ord, page, section, content) in rows {
        // 章节标题也参与打分:用户常问「第 3 章说了什么」,标题本身就是答案线索。
        let lower = format!("{section} {content}").to_lowercase();
        let mut matched = 0i64; // 命中 token 数:覆盖越多越相关
        let mut weight = 0i64; // 词长平方 × 出现次数(封顶 3):长词更具体
        let mut best = String::new();
        let mut best_len = 0usize;
        for t in tokens {
            let c = lower.matches(t.as_str()).count() as i64;
            if c > 0 {
                matched += 1;
                let len = t.chars().count() as i64;
                weight += len * len * c.min(3);
                if t.chars().count() > best_len {
                    best_len = t.chars().count();
                    best.clone_from(t);
                }
            }
        }
        if matched > 0 {
            cands.push(Cand {
                id,
                doc_name,
                ord,
                page,
                section,
                content,
                cov_score: matched * 1000 + weight,
                best,
            });
        }
    }
    cands.sort_by(|a, b| b.cov_score.cmp(&a.cov_score).then_with(|| a.ord.cmp(&b.ord)));
    Ok(cands)
}

/// 路 A:FTS5 trigram 候选(V27)。返回 (chunk id, fts rank) 按相关度升序。
/// 表不存在 / MATCH 报错时静默回退为空(旧库未迁移完成也能用)。
fn fts_candidates(
    conn: &Connection,
    session_id: &str,
    fts_q: &str,
    doc_hint: Option<&str>,
    recall_n: usize,
) -> Vec<(i64, f64)> {
    let mut sql = String::from(
        "SELECT doc_chunks_fts.rowid, rank FROM doc_chunks_fts
         JOIN doc_chunks c ON c.id = doc_chunks_fts.rowid
         WHERE doc_chunks_fts MATCH ?1 AND c.session_id = ?2",
    );
    let mut values: Vec<rusqlite::types::Value> =
        vec![
            rusqlite::types::Value::Text(fts_q.to_string()),
            rusqlite::types::Value::Text(session_id.to_string()),
        ];
    if let Some(h) = doc_hint {
        sql.push_str(" AND (c.doc_id=?3 OR c.doc_name LIKE '%'||?4||'%')");
        values.push(rusqlite::types::Value::Text(h.to_string()));
        values.push(rusqlite::types::Value::Text(h.to_string()));
    }
    // LIMIT 占位符编号随是否有 hint 变化(? 编号按出现顺序从 1 计)
    let limit_idx = values.len() + 1;
    sql.push_str(&format!(" ORDER BY rank LIMIT ?{limit_idx}"));
    values.push(rusqlite::types::Value::Integer(recall_n as i64));

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        });
    match rows {
        Ok(m) => m.collect::<Result<Vec<_>, _>>().unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// 会话内关键词检索(默认标准档策略,等价 `search_with_policy` 的 standard)。
/// 中文查询先切 2~3 字 n-gram(见 `query_tokens`),避免整句字面匹配导致召回为零。
/// 生产路径已改走 `search_with_policy`(按会话预设);本包装保留给既有测试与外部兼容。
#[cfg_attr(not(test), allow(dead_code))]
pub fn search(
    conn: &Connection,
    session_id: &str,
    query: &str,
    doc_hint: Option<&str>,
    limit: usize,
) -> Result<SearchOutcome, String> {
    search_with_policy(conn, session_id, query, doc_hint, limit, &standard_policy())
}

/// 标准档默认策略(与内置预设 standard 的 retrieval 字段一致)。
#[cfg_attr(not(test), allow(dead_code))]
fn standard_policy() -> crate::presets::RetrievalPolicy {
    crate::presets::RetrievalPolicy {
        lexical: "trigram3".into(),
        vector: true,
        rerank: false,
        top_k: 5,
    }
}

/// 混合检索(P7):
/// - 路 A = FTS5 trigram(`doc_chunks_fts`,3-gram OR 展开,见 `retrieval::fts_match_query`);
/// - 路 B = 覆盖率打分(LIKE 粗筛 + 词长权重);
/// - policy.vector=true 且两路皆非空 → RRF 融合;否则单路(A 优先,B 兜底);
/// - policy.rerank=true → 融合后 top20 按覆盖率分 + 章节命中加成软重排,再截 top_k。
pub fn search_with_policy(
    conn: &Connection,
    session_id: &str,
    query: &str,
    doc_hint: Option<&str>,
    limit: usize,
    policy: &crate::presets::RetrievalPolicy,
) -> Result<SearchOutcome, String> {
    let tokens = query_tokens(query);
    if tokens.is_empty() {
        return Ok(SearchOutcome {
            hits: Vec::new(),
            scanned: 0,
        });
    }
    let hint = doc_hint.map(str::trim).filter(|s| !s.is_empty());
    let fts_q = crate::retrieval::fts_match_query(query);
    let recall_n = if policy.rerank {
        20
    } else {
        (limit * 4).max(12)
    };

    // 路 A:FTS。查询无法展开(如 2 字词)时为空,必须靠路 B 兜底。
    let fts_hits: Vec<(i64, f64)> = match &fts_q {
        Some(q) => fts_candidates(conn, session_id, q, hint, recall_n),
        None => Vec::new(),
    };
    // 路 B:覆盖率。policy.vector=true 才作为第二路参与融合;A 为空时强制跑 B 兜底。
    let run_b = policy.vector || fts_hits.is_empty();
    let cov = if run_b {
        coverage_candidates(conn, session_id, &tokens, hint)?
    } else {
        Vec::new()
    };
    let scanned = cov.len();

    // 组装:id → Cand(FTS 命中的块按 id 回表取内容)
    // 注意:路 B 名次必须在移入 HashMap 前固定(HashMap 遍历顺序不定)。
    let cov_rank: std::collections::HashMap<i64, usize> = cov
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut by_id: std::collections::HashMap<i64, Cand> =
        cov.into_iter().map(|c| (c.id, c)).collect();
    let mut missing: Vec<i64> = Vec::new();
    for (id, _) in &fts_hits {
        if !by_id.contains_key(id) {
            missing.push(*id);
        }
    }
    if !missing.is_empty() {
        let marks = vec!["?"; missing.len()].join(",");
        let sql = format!(
            "SELECT id, doc_name, ord, page, section, content FROM doc_chunks
             WHERE session_id=? AND id IN ({marks})"
        );
        let mut values: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Text(session_id.to_string())];
        values.extend(missing.iter().map(|id| rusqlite::types::Value::Integer(*id)));
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |r| {
                Ok(Cand {
                    id: r.get(0)?,
                    doc_name: r.get(1)?,
                    ord: r.get(2)?,
                    page: r.get(3)?,
                    section: r.get(4)?,
                    content: r.get(5)?,
                    cov_score: 0,
                    best: String::new(),
                })
            });
            if let Ok(m) = rows {
                for c in m.flatten() {
                    by_id.insert(c.id, c);
                }
            }
        }
    }

    // 融合:两路皆非空 → RRF;否则按单路名次
    let fts_rank: std::collections::HashMap<i64, usize> = fts_hits
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let use_rrf = policy.vector && !fts_hits.is_empty() && !cov_rank.is_empty();
    let mut fused: Vec<(i64, f64)> = by_id
        .keys()
        .map(|id| {
            let ra = fts_rank.get(id).copied();
            let rb = cov_rank.get(id).copied();
            let s = if use_rrf {
                crate::retrieval::rrf(ra, rb)
            } else if let Some(ra) = ra {
                crate::retrieval::rrf(Some(ra), None)
            } else {
                crate::retrieval::rrf(None, rb)
            };
            (*id, s)
        })
        .collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 软重排(深度档):top20 内按覆盖率 + 章节命中微调。
    // 覆盖率归一到 [0,1] 再缩小为「一档名次」级微调(RRF 相邻名次差约 0.001-0.017),
    // 避免原始覆盖率分(数千)反过来淹没 RRF 名次。
    if policy.rerank {
        let mut top: Vec<(i64, f64)> = fused.into_iter().take(20).collect();
        let max_cov = top
            .iter()
            .filter_map(|(id, _)| by_id.get(id))
            .map(|c| c.cov_score)
            .max()
            .unwrap_or(1)
            .max(1) as f64;
        for (id, s) in top.iter_mut() {
            if let Some(c) = by_id.get(id) {
                let mut boost = 0.01 * (c.cov_score as f64 / max_cov);
                if tokens
                    .iter()
                    .any(|t| t.chars().count() >= 3 && c.section.contains(t.as_str()))
                {
                    boost += 0.015;
                }
                *s += boost;
            }
        }
        top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        fused = top;
    }

    let mut hits: Vec<SearchHit> = Vec::new();
    for (id, score) in fused.into_iter().take(limit.max(1)) {
        let Some(c) = by_id.get(&id) else { continue };
        let best = if c.best.is_empty() {
            tokens
                .iter()
                .find(|t| c.content.to_lowercase().contains(t.as_str()))
                .cloned()
                .unwrap_or_default()
        } else {
            c.best.clone()
        };
        let corpus = format!("{} {}", c.section, c.content);
        let snippet = if !best.is_empty() && c.content.to_lowercase().contains(&best) {
            snippet_of(&c.content, &best)
        } else if !best.is_empty() {
            snippet_of(&corpus, &best)
        } else {
            snippet_of(&c.content, &tokens[0])
        };
        hits.push(SearchHit {
            doc_name: c.doc_name.clone(),
            ord: c.ord,
            page: c.page,
            section: c.section.clone(),
            snippet,
            score: (score * 1_000_000.0) as i64,
        });
    }
    Ok(SearchOutcome { hits, scanned })
}

/// 解析 doc 提示(缺省且会话仅一份文档时自动选中)。
fn resolve_doc(
    conn: &Connection,
    session_id: &str,
    hint: Option<&str>,
) -> Result<Option<(String, String)>, String> {
    let docs = list_docs(conn, session_id)?;
    if let Some(h) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(docs
            .into_iter()
            .find(|d| d.doc_id == h || d.doc_name == h || d.doc_name.contains(h))
            .map(|d| (d.doc_id, d.doc_name)));
    }
    if docs.len() == 1 {
        return Ok(docs.into_iter().next().map(|d| (d.doc_id, d.doc_name)));
    }
    Ok(None)
}

/// 按块序号读取原文(带页码/章节头)。返回 (给模型的文本, 是否成功)。
pub fn read_range(
    conn: &Connection,
    session_id: &str,
    doc_hint: Option<&str>,
    start: usize,
    count: usize,
) -> Result<String, String> {
    let docs = list_docs(conn, session_id)?;
    if docs.is_empty() {
        return Err("本会话还没有驻留文档:请先上传附件".into());
    }
    let Some((doc_id, doc_name)) = resolve_doc(conn, session_id, doc_hint)? else {
        let list: Vec<String> = docs
            .iter()
            .map(|d| format!("- {} (id:{}, {} 块)", d.doc_name, d.doc_id, d.chunks))
            .collect();
        return Ok(format!(
            "会话内有 {} 份文档,请在 doc 参数中指定名称或 id:\n{}",
            docs.len(),
            list.join("\n")
        ));
    };
    let mut stmt = conn
        .prepare(
            "SELECT ord, page, section, content FROM doc_chunks
             WHERE session_id=?1 AND doc_id=?2 AND ord>=?3 AND ord<?4 ORDER BY ord",
        )
        .map_err(|e| e.to_string())?;
    let end = start.saturating_add(count.max(1)) as i64;
    let rows = stmt
        .query_map(params![session_id, doc_id, start as i64, end], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let total = docs
        .iter()
        .find(|d| d.doc_id == doc_id)
        .map(|d| d.chunks)
        .unwrap_or(0);
    let mut out = String::new();
    let mut used = 0usize;
    for row in rows {
        let (ord, page, section, content) = row.map_err(|e| e.to_string())?;
        let mut head = format!("【{} · 第 {}/{} 块", doc_name, ord + 1, total);
        if let Some(p) = page {
            head.push_str(&format!(" · 第 {p} 页"));
        }
        if !section.is_empty() {
            head.push_str(&format!(" · {section}"));
        }
        head.push_str("】\n");
        let piece = format!("{head}{content}\n\n");
        if used + piece.chars().count() > READ_MAX_CHARS {
            out.push_str(&format!(
                "…(已达单次读取上限 {READ_MAX_CHARS} 字符,剩余块请用 doc_read 续读)\n"
            ));
            break;
        }
        used += piece.chars().count();
        out.push_str(&piece);
    }
    if out.is_empty() {
        return Ok(format!(
            "`{doc_name}` 没有第 {} 块(共 {total} 块,块序号从 0 开始)",
            start
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(conn: &Connection, sid: &str) {
        conn.execute(
            "INSERT INTO sessions(id,title,created,updated) VALUES(?1,'t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            params![sid],
        )
        .unwrap();
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        doc(&conn, "s1");
        doc(&conn, "s2");
        conn
    }

    #[test]
    fn chunk_splits_by_page_and_section() {
        let text = "【第 1 页】\n摘要:这是第一段。\n\n【第 2 页】\n## 方法\n实验采用双盲设计。\n## 结果\n显著性 p<0.05。";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 3, "got: {chunks:?}");
        assert_eq!(chunks[0].page, Some(1));
        assert_eq!(chunks[1].page, Some(2));
        assert_eq!(chunks[1].section, "方法");
        assert_eq!(chunks[2].section, "结果");
        assert!(chunks[1].content.contains("双盲"));
    }

    #[test]
    fn chunk_respects_target_size() {
        // 每行 120 字 × 12 行 → 至少切成 2 块
        let line = "字".repeat(120);
        let text = [line.as_str(); 12].join("\n");
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "got {}", chunks.len());
        for c in &chunks {
            assert!(c.content.chars().count() <= CHUNK_TARGET + 200);
        }
    }

    #[test]
    fn chunk_empty_text_yields_nothing() {
        assert!(chunk_text("").is_empty());
        assert!(chunk_text("   \n\n  ").is_empty());
    }

    #[test]
    fn index_search_read_roundtrip() {
        let conn = setup();
        let text = "【第 1 页】\n## 引言\n本文研究多智能体协作。\n\n【第 2 页】\n## 方法\n我们使用检索增强生成。".to_string();
        let (_id, n) = index_document(&conn, "s1", "paper.pdf", "pdf", &text).unwrap();
        assert_eq!(n, 2);

        let docs = list_docs(&conn, "s1").unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_name, "paper.pdf");
        assert_eq!(docs[0].page_from, Some(1));
        assert_eq!(docs[0].page_to, Some(2));

        let r = search(&conn, "s1", "检索增强", None, 5).unwrap();
        assert_eq!(r.hits.len(), 1, "got: {:?}", r.hits);
        assert!(r.hits[0].snippet.contains("检索增强"));
        assert_eq!(r.hits[0].page, Some(2));

        // 读第 0 块
        let out = read_range(&conn, "s1", None, 0, 1).unwrap();
        assert!(out.contains("第 1 页"), "got: {out}");
        assert!(out.contains("多智能体协作"), "got: {out}");
    }

    #[test]
    fn reindex_replaces_old_chunks() {
        let conn = setup();
        index_document(&conn, "s1", "p.pdf", "pdf", "第一版内容").unwrap();
        index_document(&conn, "s1", "p.pdf", "pdf", "第二版内容更长了").unwrap();
        let docs = list_docs(&conn, "s1").unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].chunks, 1);
        let out = read_range(&conn, "s1", None, 0, 1).unwrap();
        assert!(out.contains("第二版"));
    }

    /// R14:同一内容重复上传(内容哈希相同)只保留一份块。
    #[test]
    fn same_content_reuses_doc_id() {
        let conn = setup();
        let (id1, _) = index_document(&conn, "s1", "a.pdf", "pdf", "同一份内容").unwrap();
        let (id2, _) = index_document(&conn, "s1", "b.pdf", "pdf", "同一份内容").unwrap();
        assert_eq!(id1, id2, "内容相同时 doc_id 必须一致");
        let docs = list_docs(&conn, "s1").unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].chunks, 1);
    }

    /// P0:中文自然语言提问必须能命中(此前整句被当成一个 token,召回为零)。
    #[test]
    fn natural_language_chinese_query_hits() {
        let conn = setup();
        let text = "【第 1 页】\n## 结论\n多智能体协作显著提升了任务完成率,实验组比对照组高 23%。";
        index_document(&conn, "s1", "report.pdf", "pdf", text).unwrap();
        let r = search(&conn, "s1", "这篇文档的主要结论是什么", None, 5).unwrap();
        assert_eq!(r.hits.len(), 1, "got: {:?}", r.hits);
        assert!(
            r.hits[0].snippet.contains("结论"),
            "got: {}",
            r.hits[0].snippet
        );
        // 命中词不应是整句
        assert!(
            query_tokens("这篇文档的主要结论是什么")
                .iter()
                .all(|t| t.chars().count() <= 4)
        );
    }

    /// 停用词不应制造误命中:问句套话单独出现时返回空。
    #[test]
    fn stopword_only_query_returns_empty() {
        let conn = setup();
        index_document(&conn, "s1", "a.pdf", "pdf", "一些普通内容").unwrap();
        let r = search(&conn, "s1", "这篇文档是什么", None, 5).unwrap();
        assert!(r.hits.is_empty(), "got: {:?}", r.hits);
    }

    #[test]
    fn session_delete_cascades_chunks() {
        let conn = setup();
        index_document(&conn, "s1", "p.pdf", "pdf", "内容").unwrap();
        index_document(&conn, "s2", "q.pdf", "pdf", "内容").unwrap();
        conn.execute("DELETE FROM sessions WHERE id='s1'", [])
            .unwrap();
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM doc_chunks WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 0, "删会话必须级联删块");
        assert_eq!(list_docs(&conn, "s2").unwrap().len(), 1);
    }

    #[test]
    fn multi_doc_read_requires_hint() {
        let conn = setup();
        index_document(&conn, "s1", "a.pdf", "pdf", "甲文档内容").unwrap();
        index_document(&conn, "s1", "b.pdf", "pdf", "乙文档内容").unwrap();
        let ambiguous = read_range(&conn, "s1", None, 0, 1).unwrap();
        assert!(
            ambiguous.contains("请在 doc 参数中指定"),
            "got: {ambiguous}"
        );
        let hit = read_range(&conn, "s1", Some("b.pdf"), 0, 1).unwrap();
        assert!(hit.contains("乙文档"), "got: {hit}");
        assert!(read_range(&conn, "missing-session", None, 0, 1).is_err());
    }
}
