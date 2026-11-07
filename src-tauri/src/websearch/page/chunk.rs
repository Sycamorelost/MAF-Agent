//! 相关段落选择:分块 → BM25 打分 → 按预算装箱。
//!
//! 目标不是「读完整个网页」,而是「把和当前问题最相关的 12k 字符先给模型」。
//! 做法对齐 Crawl4AI 的 BM25 content filter / peeky-search 的段落预算:
//! 1. 按空行切块,合并到约 900 字符(块前带上所在小标题);
//! 2. 中英混合分词(英文按词、中文按二元组),BM25 打分;
//! 3. 按「每字符得分」贪心装进预算,最相关的块排最前。
//!
//! 没命中任何关键词时退回原文开头顺序(调用方据此提示「未匹配」)。

use std::collections::{HashMap, HashSet};

/// 单块目标长度(字符)。约 900 字是检索类项目的常用值,既能覆盖一段完整论述,
/// 又不会让单个块挤占全部预算。
const TARGET_CHARS: usize = 900;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;

/// 选段结果。
pub(super) struct Selected {
    pub text: String,
    /// 是否真的命中了 query 关键词(false = 退回原文顺序)
    pub matched: bool,
}

/// 按 query 选取最相关段落,总长不超过 `budget` 字符。
pub(super) fn select(markdown: &str, query: &str, budget: usize) -> Selected {
    let chunks = build_chunks(markdown);
    if chunks.is_empty() || budget == 0 {
        return Selected {
            text: String::new(),
            matched: false,
        };
    }
    let q_tokens = unique(tokenize(query));
    if q_tokens.is_empty() {
        return Selected {
            text: pack(&chunks, &(0..chunks.len()).collect::<Vec<_>>(), budget),
            matched: false,
        };
    }

    let docs: Vec<Vec<String>> = chunks.iter().map(|c| tokenize(&c.text)).collect();
    let n = docs.len() as f32;
    let avg_len = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n;

    // 文档频率:每个词出现在多少个块里
    let mut df: HashMap<&str, usize> = HashMap::new();
    for doc in &docs {
        for tok in doc.iter().collect::<HashSet<_>>() {
            *df.entry(tok.as_str()).or_insert(0) += 1;
        }
    }

    let scores: Vec<f32> = docs
        .iter()
        .map(|doc| {
            let mut tf: HashMap<&str, usize> = HashMap::new();
            for tok in doc {
                *tf.entry(tok.as_str()).or_insert(0) += 1;
            }
            let dl = doc.len() as f32;
            q_tokens
                .iter()
                .filter_map(|q| {
                    let f = *tf.get(q.as_str())? as f32;
                    let dfq = *df.get(q.as_str())? as f32;
                    let idf = (1.0 + (n - dfq + 0.5) / (dfq + 0.5)).ln();
                    let norm = BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avg_len.max(1.0));
                    Some(idf * f * (BM25_K1 + 1.0) / (f + norm))
                })
                .sum::<f32>()
        })
        .collect();

    let mut hits: Vec<(usize, f32)> = scores
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > 0.0)
        .map(|(i, s)| (i, *s))
        .collect();
    hits.sort_by(|a, b| b.1.total_cmp(&a.1));

    if hits.is_empty() {
        return Selected {
            text: pack(&chunks, &(0..chunks.len()).collect::<Vec<_>>(), budget),
            matched: false,
        };
    }
    let order: Vec<usize> = hits.into_iter().map(|(i, _)| i).collect();
    Selected {
        text: pack(&chunks, &order, budget),
        matched: true,
    }
}

/// 一个内容块(已带小标题上下文)。
struct Chunk {
    text: String,
}

/// 分块:按空行/标题聚合到约 900 字,每个块前保留所在小标题。
fn build_chunks(markdown: &str) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut buf = String::new();
    let mut buf_len = 0usize;
    let mut heading = String::new();

    for raw in markdown.split("\n\n") {
        let block = raw.trim();
        if block.is_empty() {
            continue;
        }
        if block.starts_with('#') {
            // 标题只作为后续块的上下文,不单独占预算;换标题时先切块,
            // 保证每个块都归到正确的小标题下
            flush(&mut chunks, &mut buf, &mut buf_len, &heading);
            heading = block.lines().next().unwrap_or("").trim().to_string();
            continue;
        }
        for piece in split_block(block) {
            let piece_len = piece.chars().count();
            if buf_len > 0 && buf_len + piece_len + 2 > TARGET_CHARS {
                flush(&mut chunks, &mut buf, &mut buf_len, &heading);
            }
            if !buf.is_empty() {
                buf.push_str("\n\n");
                buf_len += 2;
            }
            buf.push_str(&piece);
            buf_len += piece_len;
            if buf_len >= TARGET_CHARS {
                flush(&mut chunks, &mut buf, &mut buf_len, &heading);
            }
        }
    }
    if !buf.trim().is_empty() {
        flush(&mut chunks, &mut buf, &mut buf_len, &heading);
    }
    chunks
}

fn flush(chunks: &mut Vec<Chunk>, buf: &mut String, buf_len: &mut usize, heading: &str) {
    let body = buf.trim();
    if body.is_empty() {
        buf.clear();
        *buf_len = 0;
        return;
    }
    let text = if heading.is_empty() || body.starts_with('#') {
        body.to_string()
    } else {
        format!("{heading}\n{body}")
    };
    chunks.push(Chunk { text });
    buf.clear();
    *buf_len = 0;
}

/// 超大段落按行切到 900 字以内;单行超长时按字符切。
fn split_block(block: &str) -> Vec<String> {
    if block.chars().count() <= TARGET_CHARS {
        return vec![block.to_string()];
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut len = 0usize;
    for line in block.lines() {
        let line = line.trim_end();
        let line_len = line.chars().count();
        if line_len > TARGET_CHARS {
            if !buf.is_empty() {
                out.push(buf.trim().to_string());
                buf.clear();
                len = 0;
            }
            let mut rest = line;
            while !rest.is_empty() {
                let take = rest
                    .char_indices()
                    .nth(TARGET_CHARS)
                    .map(|(i, _)| i)
                    .unwrap_or(rest.len());
                out.push(rest[..take].to_string());
                rest = &rest[take..];
            }
            continue;
        }
        if len > 0 && len + line_len + 1 > TARGET_CHARS {
            out.push(buf.trim().to_string());
            buf.clear();
            len = 0;
        }
        if !buf.is_empty() {
            buf.push('\n');
            len += 1;
        }
        buf.push_str(line);
        len += line_len;
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// 按给定顺序装箱,输出顺序即传入顺序(最相关的在前)。
fn pack(chunks: &[Chunk], order: &[usize], budget: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    let mut seen: HashSet<&str> = HashSet::new();
    for &i in order {
        let chunk = match chunks.get(i) {
            Some(c) => c,
            None => continue,
        };
        let text = chunk.text.trim();
        if text.is_empty() || !seen.insert(text) {
            continue;
        }
        let len = text.chars().count();
        let sep = if out.is_empty() { 0 } else { 2 };
        if used + sep + len > budget {
            // 剩余空间足够时截断装入,不够则跳过
            let remain = budget.saturating_sub(used + sep);
            if remain < 80 {
                continue;
            }
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&text.chars().take(remain).collect::<String>());
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(text);
        used += sep + len;
    }
    out
}

/// 中英混合分词:英文按 ≥2 字符的词,中文按二元组(单字成词)。
fn tokenize(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            latin.push(ch.to_ascii_lowercase());
        } else if is_cjk(ch) {
            flush_latin(&mut latin, &mut out);
            cjk.push(ch);
        } else {
            flush_latin(&mut latin, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_latin(&mut latin, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

fn flush_latin(buf: &mut String, out: &mut Vec<String>) {
    if buf.chars().count() >= 2 && !is_stopword(buf) {
        out.push(buf.clone());
    }
    buf.clear();
}

fn flush_cjk(run: &mut Vec<char>, out: &mut Vec<String>) {
    if run.len() == 1 {
        out.push(run[0].to_string());
    } else {
        for pair in run.windows(2) {
            out.push(format!("{}{}", pair[0], pair[1]));
        }
    }
    run.clear();
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
}

fn is_stopword(w: &str) -> bool {
    const WORDS: [&str; 32] = [
        "the", "and", "for", "with", "from", "this", "that", "are", "was", "were", "you", "your",
        "has", "have", "not", "but", "can", "will", "what", "when", "how", "why", "into", "then",
        "than", "they", "them", "its", "our", "out", "all", "any",
    ];
    WORDS.contains(&w)
}

fn unique(tokens: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# 安装\n\n下载安装包后双击运行,按提示下一步即可完成安装。\n\n\
# 授权\n\n完全信任模式下不再弹窗,授权是全局永久设置,重启后仍然保持。\n\n\
# 常见问题\n\n如果抓取失败,请检查网络和链接是否为公网地址。";

    #[test]
    fn picks_chunk_matching_query_and_keeps_heading() {
        let sel = select(DOC, "授权 永久", 1000);
        assert!(sel.matched);
        assert!(sel.text.contains("授权是全局永久设置"));
        assert!(sel.text.contains("# 授权"));
        assert!(!sel.text.contains("下载安装包"));
    }

    #[test]
    fn budget_is_respected() {
        let long = "相关内容".repeat(2_000);
        let sel = select(&format!("# 章节\n\n{long}"), "相关内容", 500);
        assert!(sel.text.chars().count() <= 500);
    }

    #[test]
    fn falls_back_to_head_when_nothing_matches() {
        let sel = select(DOC, "量子纠缠", 600);
        assert!(!sel.matched);
        assert!(sel.text.contains("下载安装包"));
    }

    #[test]
    fn tokenize_mixes_english_words_and_chinese_bigrams() {
        let t = tokenize("BM25 ContentFilter 提高命中率");
        assert!(t.contains(&"bm25".to_string()));
        assert!(t.contains(&"contentfilter".to_string()));
        assert!(t.contains(&"命中".to_string()));
    }
}
