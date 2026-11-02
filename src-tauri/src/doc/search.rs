//! 会话内混合检索:token 切分、FTS trigram 路、覆盖率打分路与 RRF 融合。

use rusqlite::Connection;

use crate::retrieval::strip_stopwords;

pub use super::store::{SearchHit, SearchOutcome};

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
pub(crate) struct Cand {
    pub(crate) id: i64,
    doc_name: String,
    pub(crate) ord: i64,
    page: Option<i64>,
    section: String,
    content: String,
    cov_score: i64,
    /// 命中的最长 token(用于截取片段)。
    best: String,
}

/// 路 B:覆盖率候选。SQL 层按 token 粗筛,内存按覆盖率打分。
pub(crate) fn coverage_candidates(
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
