//! 中文 AI 味结构模式(24 类,R3 B1)。
//!
//! 对应 val.md 附录 B2:逐条给出可解释的命中证据(类别 / 严重度 / 次数 / 例句),
//! 供报告页「命中规则」清单与句子热力图 hover 提示使用。

use serde::{Deserialize, Serialize};

use super::lexicon::{Lexicon, count_occ};

/// 规则命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleHit {
    pub rule_id: u32,
    pub name: String,
    pub category: String,
    /// 1 低 / 2 中 / 3 高
    pub severity: u8,
    pub count: usize,
    pub example: String,
}

/// 规则元信息。
pub struct Rule {
    pub id: u32,
    pub name: &'static str,
    pub category: &'static str,
    pub severity: u8,
}

/// 24 条规则的元信息(category ∈ 套话 / 结构 / 句式 / 词汇)。
pub const RULES: [Rule; 24] = [
    Rule {
        id: 1,
        name: "总分总模板",
        category: "结构",
        severity: 3,
    },
    Rule {
        id: 2,
        name: "段落长度均匀",
        category: "结构",
        severity: 2,
    },
    Rule {
        id: 3,
        name: "报数式行文",
        category: "结构",
        severity: 2,
    },
    Rule {
        id: 4,
        name: "排比三连",
        category: "句式",
        severity: 2,
    },
    Rule {
        id: 5,
        name: "对仗式让步",
        category: "句式",
        severity: 2,
    },
    Rule {
        id: 6,
        name: "冒号解释堆叠",
        category: "结构",
        severity: 1,
    },
    Rule {
        id: 7,
        name: "破折号插入语滥用",
        category: "句式",
        severity: 1,
    },
    Rule {
        id: 8,
        name: "长概括无实指",
        category: "结构",
        severity: 3,
    },
    Rule {
        id: 9,
        name: "递进堆叠",
        category: "句式",
        severity: 1,
    },
    Rule {
        id: 10,
        name: "对开结构",
        category: "句式",
        severity: 1,
    },
    Rule {
        id: 11,
        name: "顺序词堆叠",
        category: "套话",
        severity: 2,
    },
    Rule {
        id: 12,
        name: "随发展式开头",
        category: "套话",
        severity: 2,
    },
    Rule {
        id: 13,
        name: "在背景下句式",
        category: "套话",
        severity: 1,
    },
    Rule {
        id: 14,
        name: "插入式强调",
        category: "套话",
        severity: 1,
    },
    Rule {
        id: 15,
        name: "总结套话开头",
        category: "套话",
        severity: 2,
    },
    Rule {
        id: 16,
        name: "设问引导",
        category: "句式",
        severity: 1,
    },
    Rule {
        id: 17,
        name: "黑话堆叠",
        category: "词汇",
        severity: 2,
    },
    Rule {
        id: 18,
        name: "一是二是排列",
        category: "结构",
        severity: 1,
    },
    Rule {
        id: 19,
        name: "段首连接词",
        category: "结构",
        severity: 1,
    },
    Rule {
        id: 20,
        name: "自我指涉模板",
        category: "套话",
        severity: 1,
    },
    Rule {
        id: 21,
        name: "列表规整化",
        category: "结构",
        severity: 1,
    },
    Rule {
        id: 22,
        name: "展望式结尾",
        category: "套话",
        severity: 1,
    },
    Rule {
        id: 23,
        name: "递进强调堆叠",
        category: "句式",
        severity: 1,
    },
    Rule {
        id: 24,
        name: "数词排比",
        category: "句式",
        severity: 1,
    },
];

const SUMMARY_OPENERS: [&str; 5] = ["综上所述", "总而言之", "总体而言", "综合来看", "总之"];
const PARA_CONNECTIVES: [&str; 6] = ["此外", "同时", "因此", "然而", "而且", "另外"];
const INSERT_EMPHASIS: [&str; 3] = ["值得注意的是", "需要指出的是", "值得一提的是"];

/// 字符长度(去空白)。
fn len_no_ws(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// 截断到 n 个字符,便于展示。
pub fn trunc(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// 千字密度(不足千字按实际字数折算,避免短文密度虚高)。
fn per_kilo(n: usize, char_count: usize) -> f64 {
    if char_count == 0 {
        0.0
    } else {
        n as f64 / (char_count as f64 / 1000.0)
    }
}

/// 段落长度变异系数(CV = std / mean)。
fn para_cv(paras: &[&str]) -> f64 {
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

/// 首句长度区间(报数式行文判定:20-45 字陈述句)。
fn first_sent_len(p: &str) -> usize {
    p.split(['。', '！', '？', '!', '?', '；', ';'])
        .next()
        .map(len_no_ws)
        .unwrap_or(0)
}

/// 是否以任一前缀开头。
fn starts_with_any(s: &str, prefixes: &[&str]) -> bool {
    let t = s.trim_start();
    prefixes.iter().any(|p| t.starts_with(p))
}

/// 含任一子串。
fn contains_any(s: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| s.contains(n))
}

/// 运行全部规则;无命中的规则不返回。
pub fn run(text: &str, paras: &[&str], sents: &[String], lex: &Lexicon) -> Vec<RuleHit> {
    let char_count = len_no_ws(text);
    let mut out: Vec<RuleHit> = Vec::new();

    let mut push = |id: u32, count: usize, example: String| {
        if count == 0 {
            return;
        }
        if let Some(r) = RULES.iter().find(|r| r.id == id) {
            out.push(RuleHit {
                rule_id: r.id,
                name: r.name.to_string(),
                category: r.category.to_string(),
                severity: r.severity,
                count,
                example: trunc(&example, 50),
            });
        }
    };

    let last = paras.last().copied().unwrap_or("");
    let first = paras.first().copied().unwrap_or("");
    let cv = para_cv(paras);

    // R01 总分总模板
    if paras.len() >= 3 && starts_with_any(last, &SUMMARY_OPENERS) && len_no_ws(first) >= 80 {
        push(1, 1, tail_of(last));
    }
    // R02 段落长度均匀
    if paras.len() >= 3 && cv < 0.15 {
        push(2, 1, format!("段落长度 CV={cv:.2}"));
    }
    // R03 报数式行文(≥70% 段的段首句为 20-45 字)
    if paras.len() >= 4 {
        let n = paras
            .iter()
            .filter(|p| (20..=45).contains(&first_sent_len(p)))
            .count();
        if n as f64 / paras.len() as f64 >= 0.7 {
            push(3, n, first.to_string());
        }
    }
    // R04 排比三连
    const PAIRS: [(&str, &str); 7] = [
        ("不只是", "更是"),
        ("不仅", "更是"),
        ("不仅", "还"),
        ("不仅", "而且"),
        ("既", "又"),
        ("不但", "而且"),
        ("甚至", "更是"),
    ];
    let mut r04 = 0usize;
    let mut r04_ex = String::new();
    for s in sents {
        if PAIRS.iter().any(|(a, b)| s.contains(a) && s.contains(b)) {
            r04 += 1;
            if r04_ex.is_empty() {
                r04_ex = s.clone();
            }
        }
    }
    push(4, r04, r04_ex);

    // R05 对仗式让步:同段 ≥2 次 或 全文密度 >2/千字
    let r05_n = count_occ(text, "虽然");
    let r05_para_hit = paras.iter().any(|p| count_occ(p, "虽然") >= 2);
    if r05_para_hit || per_kilo(r05_n, char_count) > 2.0 {
        push(5, r05_n.max(2), "虽然……但是……".to_string());
    }
    // R06 冒号解释堆叠 >3/千字
    let r06 = count_occ(text, "：");
    if per_kilo(r06, char_count) > 3.0 {
        push(6, r06, first.to_string());
    }
    // R07 破折号插入语滥用 >2/千字
    let r07 = count_occ(text, "——");
    if per_kilo(r07, char_count) > 2.0 {
        push(7, r07, "——".to_string());
    }
    // R08 长概括无实指
    for p in paras {
        if len_no_ws(p) > 150
            && !p.chars().any(|c| c.is_ascii_digit())
            && !contains_any(p, &["「", "《"])
            && !p.chars().any(|c| c.is_ascii_alphabetic())
        {
            push(8, 1, p.to_string());
            break;
        }
    }
    // R09 递进堆叠 >2/千字
    let r09 = count_occ(text, "不仅");
    if per_kilo(r09, char_count) > 2.0 {
        push(9, r09, "不仅……".to_string());
    }
    // R10 对开结构
    if text.contains("一方面") && text.contains("另一方面") {
        push(10, 2, "一方面……另一方面……".to_string());
    }
    // R11 顺序词堆叠
    let seq: Vec<&str> = ["首先", "其次", "再次", "最后"]
        .into_iter()
        .filter(|w| text.contains(w))
        .collect();
    if seq.len() >= 3 {
        push(11, seq.len(), format!("{} 链", seq.join("→")));
    }
    // R12 随发展式开头
    for p in paras {
        if p.starts_with("随着") && contains_any(p, &["的发展", "的不断", "的日益"]) {
            push(12, 1, p.to_string());
            break;
        }
    }
    // R13 在背景下句式
    if text.contains('在') && contains_any(text, &["背景下", "的浪潮", "的大背景下"]) {
        push(13, 1, "在……的背景下".to_string());
    }
    // R14 插入式强调:≥1 次/500 字
    let r14: usize = INSERT_EMPHASIS.iter().map(|w| count_occ(text, w)).sum();
    if r14 > 0 && r14 as f64 / (char_count.max(1) as f64 / 500.0) >= 1.0 {
        push(14, r14, "值得注意的是".to_string());
    }
    // R15 总结套话开头(末段)
    if starts_with_any(last, &SUMMARY_OPENERS) {
        push(15, 1, tail_of(last));
    }
    // R16 设问引导
    let r16 = count_occ(text, "？");
    if r16 >= 2 && (text.contains("呢？") || text.contains("如何")) {
        push(16, r16, "？".to_string());
    }
    // R17 黑话堆叠(≥3 个不同词)
    let hype_hits: Vec<&str> = lex
        .hype
        .iter()
        .filter(|w| text.contains(w.as_str()))
        .map(|w| w.as_str())
        .collect();
    if hype_hits.len() >= 3 {
        push(17, hype_hits.len(), hype_hits.join("、"));
    }
    // R18 一是二是排列
    if count_occ(text, "一是") >= 2 && count_occ(text, "二是") >= 1 {
        push(
            18,
            count_occ(text, "一是") + count_occ(text, "二是"),
            "一是……二是……".to_string(),
        );
    }
    // R19 段首连接词
    let r19 = paras
        .iter()
        .filter(|p| starts_with_any(p, &PARA_CONNECTIVES))
        .count();
    if r19 >= 2 {
        push(19, r19, first.to_string());
    }
    // R20 自我指涉模板
    let r20: usize = ["本文", "本报告", "本方案", "本节"]
        .iter()
        .map(|w| count_occ(text, w))
        .sum();
    if r20 >= 3 {
        push(20, r20, "本文".to_string());
    }
    // R21 列表规整化
    let list_lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| {
            l.starts_with('-')
                || l.starts_with('•')
                || l.starts_with('*')
                || l.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                    && l.contains('.')
        })
        .collect();
    if list_lines.len() >= 3 {
        let lens: Vec<f64> = list_lines.iter().map(|l| len_no_ws(l) as f64).collect();
        let mean = lens.iter().sum::<f64>() / lens.len() as f64;
        if mean > 0.0 {
            let var = lens.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / lens.len() as f64;
            if var.sqrt() / mean < 0.25 {
                push(21, list_lines.len(), list_lines[0].to_string());
            }
        }
    }
    // R22 展望式结尾
    if contains_any(last, &["展望", "未来", "将会", "必将", "值得期待"]) {
        push(22, 1, tail_of(last));
    }
    // R23 递进强调堆叠 >2/千字
    let r23: usize = ["更是", "更重要", "更关键"]
        .iter()
        .map(|w| count_occ(text, w))
        .sum();
    if per_kilo(r23, char_count) > 2.0 {
        push(23, r23, "更是……".to_string());
    }
    // R24 数词排比
    const NUMERALS: [&str; 5] = ["三", "四", "五", "六", "七"];
    const NOUNS: [&str; 6] = ["维度", "要素", "关键", "原则", "要点", "路径"];
    let mut r24 = 0usize;
    let mut r24_ex = String::new();
    for n in NUMERALS {
        for n2 in ["个", "大"] {
            for w in NOUNS {
                let pat = format!("{n}{n2}{w}");
                let c = count_occ(text, &pat);
                if c > 0 {
                    r24 += c;
                    if r24_ex.is_empty() {
                        r24_ex = pat;
                    }
                }
            }
        }
    }
    if r24 >= 2 {
        push(24, r24, r24_ex);
    }

    out
}

/// 取段落末尾 40 字,便于在规则清单里标示来源。
fn tail_of(p: &str) -> String {
    let n = len_no_ws(p);
    let skip = n.saturating_sub(40);
    p.chars().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aiflavor::lexicon;

    fn hits(text: &str) -> Vec<u32> {
        let paras: Vec<&str> = text
            .split('\n')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();
        let sents: Vec<String> = text
            .split(['。', '！', '？', '!', '?', '；', ';', '\n'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        run(text, &paras, &sents, lexicon::load())
            .into_iter()
            .map(|h| h.rule_id)
            .collect()
    }

    #[test]
    fn r04_parallel_triplet() {
        assert!(hits("它不只是工具，更是伙伴，甚至是同伴。").contains(&4));
    }

    #[test]
    fn r11_sequence_stack() {
        assert!(hits("首先做调研。其次做设计。再次做验证。最后做上线。").contains(&11));
    }

    #[test]
    fn r15_summary_opener() {
        let t = "第一段先讲背景与整体情况，说明为什么需要做这件事，以及当前的现状与问题。\n\
                 中间段落展开分析，列举具体做法与依据，给出可执行的步骤与取舍。\n\
                 综上所述，整体思路是清晰的。";
        assert!(hits(t).contains(&15));
    }

    #[test]
    fn r17_hype_stack() {
        assert!(hits("我们要赋能业务，形成闭环，找到抓手。").contains(&17));
    }

    #[test]
    fn r24_numeral_parallel() {
        assert!(hits("这件事有三个要素，同时覆盖四大关键方向。").contains(&24));
    }

    #[test]
    fn clean_human_text_hits_few() {
        // 人味样本:具体数字 + 第一人称 + 口语,规则命中应很少
        let t = "昨天下午三点我在楼下便利店买了瓶水，花了三块五。\n\
                 店员是个五十来岁的大姐，边扫码边跟我抱怨说天气太热了。\n\
                 我说是啊，这鬼天气谁受得了。";
        let h = hits(t);
        assert!(h.len() <= 2, "人味样本命中过多规则:{h:?}");
    }
}
