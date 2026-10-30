//! 检索查询处理与混合排序(P7 检索升级,val.md §8 优化项 3/4)。
//!
//! 单一事实源:问句停用词与查询切词逻辑集中在本草根模块,
//! doc 检索(覆盖率打分)与全局搜索(messages/notes FTS)共用。
//!
//! 背景:FTS5 trigram 只索引 ≥3 字符的片段,2 字中文查询永远 0 命中;
//! 自然语言长问句整句 phrase 也几乎不命中。`fts_match_query` 把查询展开为
//! 3-gram OR 表达式(如「营销交付系统」→ `"营销交" OR "销交付" OR "交付系" OR "付系统"`),
//! 交由调用方在 MATCH 中使用;无可用 token 时返回 None,调用方回退旧路径(LIKE / 覆盖率)。

/// 检索停用词:问句套话与高频虚词。只剔除 ≥2 字的短语,避免把「目的」「标的」等
/// 内容词拆坏;单字虚词交给 n-gram 的权重机制自然降权。
pub(crate) const QUERY_STOPWORDS: &[&str] = &[
    "这篇", "这份", "这个", "那个", "这些", "那些", "请问", "帮我", "麻烦", "一下",
    "是什么", "什么", "怎么样", "怎么", "如何", "哪些", "哪里", "关于", "里面",
    "是否", "以及", "可以", "能够", "告诉", "总结", "概括", "内容", "正文",
    "文章", "文档", "文件", "详细", "具体", "介绍", "说明", "看看", "讲讲",
    "说说", "主要",
];

/// 去掉问句套话,保留内容词(长词优先,避免短词把长词切碎后残留噪声)。
pub(crate) fn strip_stopwords(query: &str) -> String {
    let mut q = query.to_lowercase();
    let mut words: Vec<&str> = QUERY_STOPWORDS.to_vec();
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    for w in words {
        q = q.replace(w, " ");
    }
    q
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

/// 把查询展开为 FTS5 trigram 的 MATCH 表达式(3-gram OR)。
///
/// 规则:
/// - ASCII 连续段 ≥3 字符成词;<3 不可用(trigram 索引下限)。
/// - CJK 连续段 ≥3 字 → 滑窗 3 字 gram;恰好 2 字的段不可用,返回时跳过。
/// - 每个 token 去掉双引号后加引号(phrase 语义 = 子串匹配),OR 连接。
/// - 去重,最多 12 个 token(控制 MATCH 表达式体积)。
///
/// 无可用 token(如「如何」「售前」这类 2 字查询)返回 None,调用方必须回退旧路径。
pub fn fts_match_query(query: &str) -> Option<String> {
    let cleaned = strip_stopwords(query);
    let mut tokens: Vec<String> = Vec::new();
    let mut ascii = String::new();
    let mut cjk: Vec<char> = Vec::new();
    let flush_ascii = |ascii: &mut String, tokens: &mut Vec<String>| {
        if ascii.chars().count() >= 3 {
            tokens.push(std::mem::take(ascii));
        } else {
            ascii.clear();
        }
    };
    let flush_cjk = |cjk: &mut Vec<char>, tokens: &mut Vec<String>| {
        if cjk.len() >= 3 {
            for w in cjk.windows(3) {
                tokens.push(w.iter().collect());
            }
        }
        cjk.clear();
    };
    for c in cleaned.chars() {
        if c.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut tokens);
            ascii.push(c);
        } else if is_cjk(c) {
            flush_ascii(&mut ascii, &mut tokens);
            cjk.push(c);
        } else {
            flush_ascii(&mut ascii, &mut tokens);
            flush_cjk(&mut cjk, &mut tokens);
        }
    }
    flush_ascii(&mut ascii, &mut tokens);
    flush_cjk(&mut cjk, &mut tokens);

    tokens.sort();
    tokens.dedup();
    tokens.truncate(12);
    if tokens.is_empty() {
        return None;
    }
    let expr = tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ");
    Some(expr)
}

/// Reciprocal Rank Fusion:两路排名融合,k=60(业界常用值)。
/// rank 从 0 开始;未命中传 None。分数越大越相关。
pub fn rrf(rank_a: Option<usize>, rank_b: Option<usize>) -> f64 {
    const K: f64 = 60.0;
    let mut score = 0.0;
    if let Some(r) = rank_a {
        score += 1.0 / (K + r as f64 + 1.0);
    }
    if let Some(r) = rank_b {
        score += 1.0 / (K + r as f64 + 1.0);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_cjk_query_expands_to_trigram_or() {
        let q = fts_match_query("营销交付系统").unwrap();
        // token 按字典序排序后 OR 连接
        assert_eq!(q, "\"交付系\" OR \"付系统\" OR \"营销交\" OR \"销交付\"", "got: {q}");
    }

    #[test]
    fn natural_question_strips_stopwords_then_expands() {
        // 「这篇文档的主要结论是什么」→ 剥离套话后剩「主要结论」→ wait:「主要」也是停用词 → 剩「结论」
        // 「结论」只有 2 字,不可用 → None。换一个可命中的问句:
        let q = fts_match_query("请问迁移方案的验收标准是什么").unwrap();
        // 剥离「请问」「是什么」后:「迁移方案的验收标准」→ 3-gram 滑窗
        assert!(q.contains("\"迁移方\""), "got: {q}");
        assert!(q.contains("\"验收标\""), "got: {q}");
        assert!(!q.contains("请问"), "got: {q}");
    }

    #[test]
    fn two_char_cjk_query_returns_none() {
        assert_eq!(fts_match_query("售前"), None);
        assert_eq!(fts_match_query("如何"), None);
    }

    #[test]
    fn mixed_query_keeps_long_ascii_and_cjk_grams() {
        let q = fts_match_query("API 的限流策略").unwrap();
        // 「API」3 字符可用;「的限流策略」中「的」非停用词,整段 5 字滑窗
        assert!(q.contains("\"api\""), "got: {q}");
        assert!(q.contains("\"的限流\""), "got: {q}");
    }

    #[test]
    fn short_ascii_segment_dropped() {
        // 「ab」<3 不可用;「abcdef」可用
        let q = fts_match_query("ab abcdef").unwrap();
        assert_eq!(q, "\"abcdef\"");
    }

    #[test]
    fn rrf_fusion_orders_both_hit_above_single() {
        let both = rrf(Some(0), Some(5));
        let single = rrf(Some(0), None);
        let none = rrf(None, None);
        assert!(both > single);
        assert!(single > none);
        assert_eq!(none, 0.0);
    }
}
