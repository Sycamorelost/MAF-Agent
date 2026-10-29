//! 六维统计 + 总分 + 句子级热力标注(R3 B1,val.md §5.3 / 附录 B3)。
//!
//! 全部计算在本地完成,不调用任何模型;只回答「哪里的写作特征像 AI」,
//! 不作「人写 / AI 写」的二值判定(伦理要求见 `DISCLAIMER`)。

use serde::{Deserialize, Serialize};

use super::lexicon::{self, count_occ};
use super::rules::{self, RuleHit};

/// 引擎版本:词表/算法变更时递增,报告里随结果落库,便于对比历史。
pub const ENGINE_VERSION: u8 = 1;

/// 固定免责声明(报告页与导出必带)。
pub const DISCLAIMER: &str = "AI 概率是写作特征统计,不是抄袭或代写的证明。检测器对非母语写作、模板化文体存在系统性误报(斯坦福研究:平均误报 61.2%)。";

/// 六维单维评分。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimScore {
    /// cliche / burstiness / lexical / connective / concrete / template
    pub key: String,
    /// 套话密度 / 句长均匀度 / 词汇多样性 / 连接词密度 / 具体性 / 结构模板度
    pub name: String,
    /// 人类可读原始值
    pub raw: String,
    /// 0-100(0=人味,100=AI 味)
    pub score: u32,
    /// 0 正常 / 1 轻度 / 2 中度 / 3 高度
    pub level: u8,
    pub example: String,
}

/// 句子级热力标注。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceMark {
    pub index: usize,
    pub text: String,
    pub score: u32,
    /// 0..3 四级热力
    pub heat: u8,
    pub reasons: Vec<String>,
}

/// 引擎原始统计值(报告页「原始数据」折叠区)。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineStats {
    pub char_count: usize,
    pub sentence_count: usize,
    pub paragraph_count: usize,
    pub cliche_per_kilo: f64,
    pub sentence_len_mean: f64,
    pub sentence_len_std: f64,
    pub lexical_diversity: f64,
    pub connective_per_kilo: f64,
    pub concrete_per_kilo: f64,
    pub para_len_cv: f64,
    pub academic_style: bool,
}

/// 完整检测报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorReport {
    /// 落库后回填
    pub id: Option<i64>,
    pub created: Option<String>,
    pub score: u32,
    /// 低 / 中 / 偏高 / 高 / 极高
    pub level: String,
    /// 0-4
    pub level_code: u8,
    pub char_count: usize,
    /// 短文(≤100 字)→ 结果仅供参考,level 封顶「中」
    pub degraded: bool,
    pub dims: Vec<DimScore>,
    pub sentence_marks: Vec<SentenceMark>,
    pub rule_hits: Vec<RuleHit>,
    pub stats: EngineStats,
    pub disclaimer: String,
    pub version: u8,
}

// ============ 基础工具 ============

fn len_no_ws(s: &str) -> usize {
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
fn interp(x: f64, pts: &[(f64, f64)]) -> f64 {
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

fn dim_level(score: u32) -> u8 {
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

fn heat_of(score: u32) -> u8 {
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
fn para_cv(paras: &[String]) -> f64 {
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
fn tokens(text: &str) -> Vec<String> {
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
fn concrete_signals(text: &str) -> usize {
    const FP: [&str; 8] = ["我", "我们", "昨天", "今天", "上周", "去年", "刚才", "早上"];
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
    // 第一人称事件标记
    for w in FP {
        n += count_occ(text, w);
    }
    n
}

/// 学术文体识别:命中 ≥3 个学术信号即视为学术文本(具体性维度放宽)。
fn is_academic(text: &str) -> bool {
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

// ============ 主入口 ============

/// 空文本的六维占位(全部 0,不参与评分)。
fn zero_dims() -> Vec<DimScore> {
    const SPEC: [(&str, &str); 6] = [
        ("cliche", "套话密度"),
        ("burstiness", "句长均匀度"),
        ("lexical", "词汇多样性"),
        ("connective", "连接词密度"),
        ("concrete", "具体性"),
        ("template", "结构模板度"),
    ];
    SPEC.iter()
        .map(|(k, n)| DimScore {
            key: (*k).to_string(),
            name: (*n).to_string(),
            raw: "无内容".into(),
            score: 0,
            level: 0,
            example: String::new(),
        })
        .collect()
}

/// 对文本运行本地统计,产出结构化报告。
pub fn analyze(text: &str) -> FlavorReport {
    let lex = lexicon::load();
    let char_count = len_no_ws(text);
    // 空文本直接返回零值报告(避免各维度除零后出现「无内容 = 高分」的荒谬结果)
    if char_count == 0 {
        return FlavorReport {
            id: None,
            created: None,
            score: 0,
            level: "低".into(),
            level_code: 0,
            char_count: 0,
            degraded: true,
            dims: zero_dims(),
            sentence_marks: Vec::new(),
            rule_hits: Vec::new(),
            stats: EngineStats::default(),
            disclaimer: DISCLAIMER.to_string(),
            version: ENGINE_VERSION,
        };
    }
    let sents = split_sentences(text);
    let paras = split_paragraphs(text);
    let para_refs: Vec<&str> = paras.iter().map(|p| p.as_str()).collect();
    let hits = rules::run(text, &para_refs, &sents, lex);

    // —— 句长统计 ——
    let lens: Vec<f64> = sents.iter().map(|s| len_no_ws(s) as f64).collect();
    let mean = if lens.is_empty() {
        0.0
    } else {
        lens.iter().sum::<f64>() / lens.len() as f64
    };
    let std = if lens.is_empty() {
        0.0
    } else {
        (lens.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / lens.len() as f64).sqrt()
    };

    // —— 套话 / 黑话密度 ——
    let mut cliche_hits = 0usize;
    let mut cliche_example = String::new();
    for w in &lex.cliche {
        let c = count_occ(text, w);
        if c > 0 {
            cliche_hits += c;
            if cliche_example.is_empty() {
                cliche_example = format!("{}×{c}", w);
            }
        }
    }
    for w in &lex.hype {
        cliche_hits += count_occ(text, w);
    }
    let cliche_per_kilo = per_kilo(cliche_hits, char_count);

    // —— 连接词密度 ——
    let mut conn_hits = 0usize;
    let mut conn_example = String::new();
    for w in &lex.connectives {
        let c = count_occ(text, w);
        if c > 0 {
            conn_hits += c;
            if conn_example.is_empty() {
                conn_example = format!("{}×{c}", w);
            }
        }
    }
    let connective_per_kilo = per_kilo(conn_hits, char_count);

    // —— 词汇多样性(代理) ——
    let toks = tokens(text);
    let lexical_diversity = if toks.is_empty() {
        0.0
    } else {
        let mut uniq = toks.clone();
        uniq.sort();
        uniq.dedup();
        uniq.len() as f64 / toks.len() as f64
    };

    // —— 具体性 ——
    let concrete = concrete_signals(text);
    let concrete_per_kilo = per_kilo(concrete, char_count);
    let academic_style = is_academic(text);

    // —— 结构模板度(三信号 + 规则补充) ——
    let cv = para_cv(&paras);
    let mut tmpl = 0u32;
    if cv < 0.15 {
        tmpl += 30;
    }
    // 段首 4 字重复
    let mut prefixes: Vec<String> = Vec::new();
    for p in &paras {
        prefixes.push(p.chars().take(4).collect());
    }
    let mut max_rep = 0usize;
    for (i, p) in prefixes.iter().enumerate() {
        if p.chars().count() < 2 {
            continue;
        }
        let c = prefixes.iter().skip(i).filter(|q| *q == p).count();
        max_rep = max_rep.max(c);
    }
    if max_rep >= 2 {
        tmpl += 30;
    }
    const SUMMARY_OPENERS: [&str; 5] = ["综上所述", "总而言之", "总体而言", "综合来看", "总之"];
    if let Some(last) = paras.last()
        && SUMMARY_OPENERS.iter().any(|w| last.starts_with(w))
    {
        tmpl += 30;
    }
    // 结构类规则补充(每命中 1 条 +8,最多 +16)
    let struct_hits = hits.iter().filter(|h| h.category == "结构").count() as u32;
    tmpl += (struct_hits * 8).min(16);
    let tmpl_score = tmpl.min(95);

    // —— 六维评分 ——
    let cliche_score = interp(
        cliche_per_kilo,
        &[(1.0, 20.0), (3.0, 55.0), (6.0, 85.0), (10.0, 95.0)],
    ) as u32;
    let burst_score = if sents.len() < 5 {
        30
    } else {
        interp(std, &[(5.0, 85.0), (6.5, 65.0), (8.0, 40.0), (10.0, 12.0)]) as u32
    };
    let adj = if char_count < 1000 { 0.04 } else { 0.0 };
    let lex_pts = [(0.25 + adj, 85.0), (0.34 + adj, 65.0), (0.45 + adj, 15.0)];
    // 词形样本不足(<5 个 token)时固定 30:避免空文本被判成「词汇多样性低 = AI 味」
    let lexical_score = if toks.len() < 5 {
        30
    } else {
        interp(lexical_diversity, &lex_pts) as u32
    };
    let conn_score = interp(
        connective_per_kilo,
        &[(2.0, 15.0), (4.0, 40.0), (8.0, 65.0), (14.0, 85.0)],
    ) as u32;
    let concrete_score = interp(concrete_per_kilo, &[(0.5, 80.0), (1.0, 55.0), (3.0, 18.0)]) as u32;
    let concrete_score = if academic_style {
        concrete_score.min(45)
    } else {
        concrete_score
    };

    let dims = vec![
        DimScore {
            key: "cliche".into(),
            name: "套话密度".into(),
            raw: format!("{cliche_per_kilo:.1} 次/千字"),
            score: cliche_score,
            level: dim_level(cliche_score),
            example: cliche_example,
        },
        DimScore {
            key: "burstiness".into(),
            name: "句长均匀度".into(),
            raw: if sents.len() < 5 {
                format!("样本不足({} 句)", sents.len())
            } else {
                format!("σ={std:.1} 字,均值 {mean:.1}")
            },
            score: burst_score,
            level: dim_level(burst_score),
            example: sents
                .first()
                .map(|s| rules::trunc(s, 40))
                .unwrap_or_default(),
        },
        DimScore {
            key: "lexical".into(),
            name: "词汇多样性".into(),
            raw: if char_count < 1000 {
                format!("{lexical_diversity:.2}(短文放宽)")
            } else {
                format!("{lexical_diversity:.2}")
            },
            score: lexical_score,
            level: dim_level(lexical_score),
            example: String::new(),
        },
        DimScore {
            key: "connective".into(),
            name: "连接词密度".into(),
            raw: format!("{connective_per_kilo:.1} 次/千字"),
            score: conn_score,
            level: dim_level(conn_score),
            example: conn_example,
        },
        DimScore {
            key: "concrete".into(),
            name: "具体性".into(),
            raw: if academic_style {
                format!("{concrete_per_kilo:.1} 次/千字 · 学术放宽")
            } else {
                format!("{concrete_per_kilo:.1} 次/千字")
            },
            score: concrete_score,
            level: dim_level(concrete_score),
            example: String::new(),
        },
        DimScore {
            key: "template".into(),
            name: "结构模板度".into(),
            raw: format!("段落 CV={cv:.2},模板信号 {struct_hits} 条"),
            score: tmpl_score,
            level: dim_level(tmpl_score),
            example: paras
                .last()
                .map(|p| rules::trunc(p, 40))
                .unwrap_or_default(),
        },
    ];

    // —— 总分:六维加权 + 规则加成 ——
    const W: [f64; 6] = [0.25, 0.20, 0.15, 0.15, 0.15, 0.10];
    let base: f64 = dims
        .iter()
        .zip(W.iter())
        .map(|(d, w)| d.score as f64 * w)
        .sum();
    let sev_sum: usize = hits.iter().map(|h| h.severity as usize * h.count).sum();
    let bonus = ((sev_sum as f64) * 0.10).min(12.0);
    let total = (base + bonus).round().clamp(0.0, 100.0) as u32;

    // —— 五级 + 短文降级 ——
    let (mut level, mut level_code) = if total < 30 {
        ("低", 0u8)
    } else if total < 55 {
        ("中", 1)
    } else if total < 75 {
        ("偏高", 2)
    } else if total < 90 {
        ("高", 3)
    } else {
        ("极高", 4)
    };
    let degraded = char_count <= 100;
    if degraded && level_code > 1 {
        level = "中";
        level_code = 1;
    }

    // —— 句子热力标注 ——
    let sentence_marks = sents
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut score = 0u32;
            let mut reasons: Vec<String> = Vec::new();
            for w in &lex.cliche {
                if s.contains(w.as_str()) {
                    score += 30;
                    reasons.push(format!("套话「{w}」"));
                    break;
                }
            }
            for w in &lex.hype {
                if s.contains(w.as_str()) {
                    score += 25;
                    reasons.push(format!("黑话「{w}」"));
                    break;
                }
            }
            for w in &lex.connectives {
                if s.contains(w.as_str()) {
                    score += 15;
                    reasons.push(format!("连接词「{w}」"));
                    break;
                }
            }
            let l = len_no_ws(s) as f64;
            if sents.len() >= 5 && (l - mean).abs() < 1.5 {
                score += 10;
                reasons.push("句长接近全文均值".into());
            }
            let score = score.min(100);
            reasons.truncate(3);
            SentenceMark {
                index: i,
                text: s.clone(),
                score,
                heat: heat_of(score),
                reasons,
            }
        })
        .collect();

    FlavorReport {
        id: None,
        created: None,
        score: total,
        level: level.to_string(),
        level_code,
        char_count,
        degraded,
        dims,
        sentence_marks,
        rule_hits: hits,
        stats: EngineStats {
            char_count,
            sentence_count: sents.len(),
            paragraph_count: paras.len(),
            cliche_per_kilo,
            sentence_len_mean: mean,
            sentence_len_std: std,
            lexical_diversity,
            connective_per_kilo,
            concrete_per_kilo,
            para_len_cv: cv,
            academic_style,
        },
        disclaimer: DISCLAIMER.to_string(),
        version: ENGINE_VERSION,
    }
}

/// 千字密度。
fn per_kilo(n: usize, char_count: usize) -> f64 {
    if char_count == 0 {
        0.0
    } else {
        n as f64 / (char_count as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 典型「AI 味」样本:套话 + 连接词堆叠 + 均匀短句 + 总结开头。
    const AI_SAMPLE: &str = "随着人工智能技术的不断发展，越来越多的行业开始关注这一领域。\
众所周知，技术是推动社会进步的重要力量。首先，我们需要明确目标。其次，我们需要制定方案。\
再次，我们需要落实执行。最后，我们需要总结反思。与此同时，值得注意的是，团队协作同样重要。\
因此，我们要加强沟通。此外，我们还要完善机制。综上所述，只有不断努力，才能取得更好的成果。\
总而言之，未来的发展前景十分广阔。不可否认，这条路并不容易。毋庸置疑，坚持就是胜利。";

    /// 人味样本:口语、具体数字、第一人称、长短句交错。
    const HUMAN_SAMPLE: &str = "昨天下午三点我在楼下便利店买了瓶水，花了三块五。\
店员是个五十来岁的大姐，边扫码边跟我抱怨说天气太热了，空调又坏了半天没人修。\
我说是啊。她叹了口气，说这鬼天气谁受得了，然后顺手塞给我一颗薄荷糖，说含着凉快。\
我愣了一下，道了谢。出门的时候太阳晃得眼睛疼，我就站在门口的阴影里把水喝完，\
看着她继续招呼下一个客人。";

    #[test]
    fn ai_sample_scores_higher_than_human() {
        let ai = analyze(AI_SAMPLE);
        let hm = analyze(HUMAN_SAMPLE);
        assert!(
            ai.score >= 60,
            "AI 样本得分应 ≥60,实际 {} ({:?})",
            ai.score,
            ai.rule_hits.iter().map(|h| h.rule_id).collect::<Vec<_>>()
        );
        assert!(hm.score <= 45, "人味样本得分应 ≤45,实际 {}", hm.score);
        assert!(
            ai.score >= hm.score + 25,
            "样本区分度不足:{} vs {}",
            ai.score,
            hm.score
        );
    }

    #[test]
    fn dims_order_and_keys() {
        let r = analyze(AI_SAMPLE);
        let keys: Vec<&str> = r.dims.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "cliche",
                "burstiness",
                "lexical",
                "connective",
                "concrete",
                "template"
            ]
        );
        let names: Vec<&str> = r.dims.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "套话密度",
                "句长均匀度",
                "词汇多样性",
                "连接词密度",
                "具体性",
                "结构模板度"
            ]
        );
    }

    #[test]
    fn short_text_degraded_and_capped() {
        let r = analyze("综上所述，这件事很重要。");
        assert!(r.degraded, "≤100 字应标记 degraded");
        assert!(
            r.level_code <= 1,
            "短文 level 应封顶「中」,实际 {}",
            r.level_code
        );
        assert_eq!(r.level, "中");
        // 短文句数少,burstiness 固定 30
        let burst = r.dims.iter().find(|d| d.key == "burstiness").unwrap();
        assert_eq!(burst.score, 30);
    }

    #[test]
    fn academic_text_relaxes_concrete() {
        let t = "本研究参考 Smith et al 的方法，实验组与对照组各 30 例，显著性水平 p<0.05。\
结果见图1 与表1，差异具有统计学意义。参考文献 [1] 给出了对照标准。\
实验组在第三周出现明显改善，且未观察到不良反应。";
        let r = analyze(t);
        assert!(r.stats.academic_style, "应识别为学术文体");
        let c = r.dims.iter().find(|d| d.key == "concrete").unwrap();
        assert!(c.score <= 45, "学术文本具体性应封顶 45,实际 {}", c.score);
    }

    #[test]
    fn sentence_marks_cover_all_sentences() {
        let r = analyze(AI_SAMPLE);
        assert_eq!(r.sentence_marks.len(), r.stats.sentence_count);
        assert!(
            r.sentence_marks
                .iter()
                .all(|m| m.heat <= 3 && m.reasons.len() <= 3)
        );
        // 含「首先」的句子应至少轻度
        assert!(
            r.sentence_marks
                .iter()
                .any(|m| m.text.contains("首先") && m.score > 0)
        );
    }

    #[test]
    fn empty_text_is_safe() {
        let r = analyze("");
        assert_eq!(r.char_count, 0);
        assert_eq!(r.score, 0);
        assert_eq!(r.level, "低");
        assert!(r.sentence_marks.is_empty());
    }

    #[test]
    fn has_disclaimer_and_version() {
        let r = analyze("测试一句。");
        assert!(r.disclaimer.contains("61.2"));
        assert_eq!(r.version, ENGINE_VERSION);
        assert!(r.id.is_none() && r.created.is_none(), "引擎不负责落库字段");
    }
}
