//! 去 AI 味引擎:基础统计工具 + V3 动态统计 proxy。
//!
//! 职责拆分:
//! - `engine/mod.rs` — 类型定义 + 主入口 `analyze()` + 总分加权
//! - `engine/stats.rs` — 基础统计函数 + V3 动态统计(本文件)
//!
//! 所有函数纯函数,无状态,方便单测独立调用。

// ============ 基础工具 ============

pub fn len_no_ws(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// 按句末标点切句(保留标点,连续标点合并为一句)。
pub fn split_sentences(text: &str) -> Vec<String> {
    const ENDS: [char; 9] = ['。', '！', '？', '!', '?', '…', '；', ';', '\n'];
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if ENDS.contains(&ch) {
            let t = cur.trim().to_string();
            if !t.is_empty() {
                out.push(t);
            }
            cur.clear();
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

/// 分段(按换行,过滤空行) —— 返回拥有的字符串,避免生命周期外溢。
pub fn split_paragraphs(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// 分段线性插值:`pts` 按 x 升序,返回在两端夹取后的 y。
pub fn interp(x: f64, pts: &[(f64, f64)]) -> f64 {
    if pts.is_empty() {
        return 0.0;
    }
    if x <= pts[0].0 {
        return pts[0].1;
    }
    if x >= pts[pts.len() - 1].0 {
        return pts[pts.len() - 1].1;
    }
    for w in pts.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if x >= x0 && x <= x1 {
            let r = if (x1 - x0).abs() < f64::EPSILON {
                0.0
            } else {
                (x - x0) / (x1 - x0)
            };
            return y0 + (y1 - y0) * r;
        }
    }
    pts[pts.len() - 1].1
}

pub fn dim_level(score: u32) -> u8 {
    if score < 30 {
        0
    } else if score < 55 {
        1
    } else if score < 75 {
        2
    } else {
        3
    }
}

pub fn heat_of(score: u32) -> u8 {
    if score < 30 {
        0
    } else if score < 55 {
        1
    } else if score < 75 {
        2
    } else {
        3
    }
}

/// 段落长度变异系数。
pub fn para_cv(paras: &[String]) -> f64 {
    if paras.len() < 2 {
        return 0.0;
    }
    let lens: Vec<f64> = paras.iter().map(|p| len_no_ws(p) as f64).collect();
    let mean = lens.iter().sum::<f64>() / lens.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let var = lens.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / lens.len() as f64;
    var.sqrt() / mean
}

/// 词形统计的代理指标准:ASCII 词(≥2 字符) + CJK 二元组。
pub fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    // ASCII 词
    let mut buf = String::new();
    for c in &chars {
        if c.is_ascii_alphanumeric() {
            buf.push(c.to_ascii_lowercase());
        } else if !buf.is_empty() {
            if buf.chars().count() >= 2 {
                out.push(buf.clone());
            }
            buf.clear();
        }
    }
    if buf.chars().count() >= 2 {
        out.push(buf);
    }
    // CJK 二元组(仅取汉字)
    let han: Vec<char> = chars
        .iter()
        .copied()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .collect();
    for w in han.windows(2) {
        out.push(format!("{}{}", w[0], w[1]));
    }
    out
}

/// 具体性信号计数:数字、引号/书名号、ASCII 词、第一人称事件标记。
/// NOTE:故意排除"我们"——AI 爱用"我们得/我们要",那是 AI 信号不是具体性。
pub fn concrete_signals(text: &str) -> usize {
    use crate::aiflavor::lexicon::count_occ;
    const FP: [&str; 7] = ["我", "昨天", "今天", "上周", "去年", "刚才", "早上"];
    let mut n = 0usize;
    // 数字串
    let mut in_num = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_num {
                n += 1;
                in_num = true;
            }
        } else {
            in_num = false;
        }
    }
    // 引号 / 书名号
    n += count_occ(text, "「") + count_occ(text, "《");
    // ASCII 词(≥2)
    let mut buf = 0usize;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            buf += 1;
        } else {
            if buf >= 2 {
                n += 1;
            }
            buf = 0;
        }
    }
    if buf >= 2 {
        n += 1;
    }
    // 第一人称事件标记(不含"我们"里的"我")
    for w in FP {
        let c = if w == "我" {
            let mut n2 = 0;
            let chars: Vec<char> = text.chars().collect();
            for i in 0..chars.len() {
                if chars[i] == '我'
                    && (i + 1 >= chars.len() || chars[i + 1] != '们')
                {
                    n2 += 1;
                }
            }
            n2
        } else {
            count_occ(text, w)
        };
        n += c;
    }
    n
}

/// 学术文体识别:命中 ≥3 个学术信号即视为学术文本(具体性维度放宽)。
pub fn is_academic(text: &str) -> bool {
    const SIG: [&str; 9] = [
        "et al",
        "参考文献",
        "[1]",
        "图1",
        "表1",
        "实验组",
        "显著性",
        "p<",
        "p =",
    ];
    SIG.iter().filter(|s| text.contains(*s)).count() >= 3
}

/// 千字密度。
pub fn per_kilo(n: usize, char_count: usize) -> f64 {
    if char_count == 0 {
        0.0
    } else {
        n as f64 / (char_count as f64 / 1000.0)
    }
}

// ============ V3 动态统计 proxy(完全不依赖词表,动态分析) ============

/// CJK 二元组频率分布的变异系数(CV = std / mean)。
///
/// **Perplexity proxy 原理**:AI 文本的 bigram 频率更集中(模型反复用同样搭配),
/// CV 高。人类文本词汇分散,CV 低。
///
/// 返回 (cv, unique_bigrams, total_bigrams) —— cv ∈ [0, 2+]。
pub fn bigram_freq_cv(text: &str) -> (f64, usize, usize) {
    let han: Vec<char> = text
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .collect();
    if han.len() < 4 {
        return (0.0, 0, 0);
    }
    let mut freq: std::collections::HashMap<[char; 2], usize> = std::collections::HashMap::new();
    let mut total = 0usize;
    for w in han.windows(2) {
        *freq.entry([w[0], w[1]]).or_insert(0) += 1;
        total += 1;
    }
    if freq.len() < 3 {
        return (0.0, freq.len(), total);
    }
    let values: Vec<f64> = freq.values().map(|&v| v as f64).collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean <= 0.0 {
        return (0.0, freq.len(), total);
    }
    let var = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let cv = var.sqrt() / mean;
    (cv, freq.len(), total)
}

/// 单个句子内 CJK bigram 的香农熵:H = -Σ p(b) * log2 p(b)。
pub fn sentence_bigram_entropy(sent: &str) -> f64 {
    let han: Vec<char> = sent
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .collect();
    if han.len() < 4 {
        return 0.0;
    }
    let mut freq: std::collections::HashMap<[char; 2], usize> = std::collections::HashMap::new();
    let mut total = 0usize;
    for w in han.windows(2) {
        *freq.entry([w[0], w[1]]).or_insert(0) += 1;
        total += 1;
    }
    if freq.is_empty() || total < 4 {
        return 0.0;
    }
    let mut h = 0.0;
    for &c in freq.values() {
        let p = c as f64 / total as f64;
        if p > 0.0 {
            h -= p * p.log2();
        }
    }
    h
}

/// 句子级熵的变异系数(CV)。
///
/// **Entropy variance proxy 原理**:AI 写作每个句子的熵都差不多
/// (模型均匀填充),因此跨句熵 CV 低。人类写作长短句交错、
/// 口语与书面混用,各句熵差异大,CV 高。
pub fn sentence_entropy_cv(sents: &[String]) -> f64 {
    let entropies: Vec<f64> = sents
        .iter()
        .map(|s| sentence_bigram_entropy(s))
        .filter(|&e| e > 0.0)
        .collect();
    if entropies.len() < 3 {
        return 0.0;
    }
    let mean = entropies.iter().sum::<f64>() / entropies.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let var = entropies
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>()
        / entropies.len() as f64;
    var.sqrt() / mean
}

/// 标点韵律间隔的变异系数(CV)。
///
/// **Rhythm proxy 原理**:AI 写作节奏匀速(模型 next-token 选择
/// 概率分布平滑导致相邻标点间隔接近常数),CV 低。人类写作
/// 有长有短、有快有慢,CV 高。
///
/// 统计句号/逗号/顿号/感叹号/问号的字符 offset,计算相邻间隔。
pub fn punct_rhythm_cv(text: &str) -> f64 {
    const PUNCTS: [char; 10] = [
        '。', '！', '？', '，', '、', '!', '?', '.', ',', ';',
    ];
    let mut offsets: Vec<usize> = Vec::new();
    for (i, c) in text.chars().enumerate() {
        if PUNCTS.contains(&c) {
            offsets.push(i);
        }
    }
    if offsets.len() < 5 {
        return 0.0;
    }
    let mut intervals: Vec<f64> = Vec::new();
    for w in offsets.windows(2) {
        intervals.push((w[1] - w[0]) as f64);
    }
    if intervals.len() < 4 {
        return 0.0;
    }
    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let var = intervals
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>()
        / intervals.len() as f64;
    var.sqrt() / mean
}
