//! AI 检测引擎主入口(R3 B1)。
//!
//! 职责拆分(见 mod.rs 底部 stats 模块注释):
//! - 本文件:类型定义 + 主入口 `analyze()` + 总分加权 + 句子热力标注 + 测试
//! - `stats.rs`:基础统计函数 + V3 动态统计 proxy(纯函数,方便单测)
//!
//! 全部计算在本地完成,不调用任何模型;只回答「哪里的写作特征像 AI」,
//! 不作「人写 / AI 写」的二值判定(伦理要求见 `DISCLAIMER`)。

mod stats;

use serde::{Deserialize, Serialize};

use super::lexicon::{self, count_occ};
use super::rules::{self, RuleHit};

pub use stats::*;

/// 引擎版本:词表/算法变更时递增,报告里随结果落库,便于对比历史。
/// V3:新增三个不依赖任何词表的动态统计维度——
///   - perplexity_proxy:二元组频率分布变异系数(AI 文本频率更均匀)
///   - sentence_entropy_cv:句子内二元组熵的变异系数(AI 各句熵接近)
///   - punct_rhythm_cv:标点韵律间隔变异系数(AI 节奏匀速)
pub const ENGINE_VERSION: u8 = 3;

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

// ============ 主入口 ============

/// 空文本的九维占位(全部 0,不参与评分)。
fn zero_dims() -> Vec<DimScore> {
    const SPEC: [(&str, &str); 9] = [
        ("cliche", "套话密度"),
        ("burstiness", "句长均匀度"),
        ("perplexity", "频率均匀度"),
        ("lexical", "词汇多样性"),
        ("entropy_var", "句熵变异度"),
        ("connective", "连接词密度"),
        ("concrete", "具体性"),
        ("punct_rhythm", "标点节奏"),
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

    // —— 句长统计(均值 + 标准差 + 相邻变化率 delta) ——
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
    // 相邻句长变化率:AI 写作匀速(平均 delta 小),人写的长短句交替(delta 大)
    let delta_mean: f64 = if lens.len() < 2 {
        0.0
    } else {
        lens.windows(2).map(|w| (w[0] - w[1]).abs()).sum::<f64>() / (lens.len() - 1) as f64
    };

    // —— 标点密度 proxy(不依赖任何词) ——
    // AI 文本连接词多 → 逗号密集;人写的短句多 → 句号比例高
    let commas = count_occ(text, "，") + count_occ(text, ",");
    let stops = count_occ(text, "。") + count_occ(text, ".") + count_occ(text, "！") + count_occ(text, "！");
    let stop_total = commas + stops;
    let comma_ratio = if stop_total == 0 {
        0.5 // 无标点按中性处理
    } else {
        commas as f64 / stop_total as f64
    };

    // —— 套话 / 黑话密度(词表信号) ——
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

    // —— 连接词密度(词表信号) ——
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
    // 第一人称具体事件词密度 proxy(AI 极少用"我/昨天/去年"等亲身经历词)
    const FP: [&str; 7] = ["我", "昨天", "今天", "上周", "去年", "刚才", "早上"];
    let fp_count: usize = FP.iter().map(|w| count_occ(text, w)).sum();
    let fp_per_kilo = per_kilo(fp_count, char_count);
    // "我们"密度 penalty(AI 爱用集体主语"我们得/我们要")
    let we_count = count_occ(text, "我们");
    let we_per_kilo = per_kilo(we_count, char_count);

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

    // —— 六维评分:词表信号 ∪ 不依赖词的统计 proxy ——
    // cliche 基底:词表命中(换同义词会变低),但「空泛 proxy」兜底——完全没具体信号的文本 = 大概率 AI
    let cliche_score_word = interp(
        cliche_per_kilo,
        &[(1.0, 20.0), (3.0, 55.0), (6.0, 85.0), (10.0, 95.0)],
    );
    let cliche_score_empty_proxy = if char_count <= 100 {
        // 短文没空间写具体内容,空泛 proxy 跳过(短文已经有 degraded 降级保护)
        0.0
    } else if concrete_per_kilo < 0.3 {
        // 千字不到 0.3 个具体信号 → 高度空泛,AI 基底 65 分(换同义词也逃不掉)
        65.0
    } else if concrete_per_kilo < 1.0 {
        40.0
    } else {
        15.0
    };
    let cliche_score = cliche_score_word.max(cliche_score_empty_proxy) as u32;

    // burstiness:句长均匀度。delta_mean 小 → 匀速 → AI 味(不依赖任何词)
    let burst_score_word = if sents.len() < 5 {
        30.0
    } else {
        interp(std, &[(5.0, 85.0), (6.5, 65.0), (8.0, 40.0), (10.0, 12.0)])
    };
    // delta_mean proxy:AI 相邻句长差 < 3,人写的通常 > 6
    // 至少 3 句才有"变化率"的意义,单/双句跳过
    let delta_proxy = if lens.len() < 3 {
        0.0
    } else if delta_mean < 1.5 {
        25.0
    } else if delta_mean < 3.0 {
        15.0
    } else {
        0.0
    };
    let burst_score = (burst_score_word + delta_proxy).min(95.0) as u32;

    let adj = if char_count < 1000 { 0.04 } else { 0.0 };
    let lex_pts = [(0.25 + adj, 85.0), (0.34 + adj, 65.0), (0.45 + adj, 15.0)];
    let lexical_score = if toks.len() < 5 {
        30
    } else {
        interp(lexical_diversity, &lex_pts) as u32
    };

    // connective 基底:词表 + 标点密度 proxy(逗号比例 > 0.5 = AI 长句多)
    let conn_score_word = interp(
        connective_per_kilo,
        &[(2.0, 15.0), (4.0, 40.0), (8.0, 65.0), (14.0, 85.0)],
    );
    let conn_score_punct_proxy = if comma_ratio > 0.6 {
        55.0
    } else if comma_ratio > 0.45 {
        30.0
    } else {
        10.0
    };
    let conn_score = conn_score_word.max(conn_score_punct_proxy) as u32;

    // concrete:具体性。第一人称事件词少 → 加 penalty;集体主语"我们"多 → 也加 penalty(AI 信号)
    let concrete_score = interp(concrete_per_kilo, &[(0.5, 80.0), (1.0, 55.0), (3.0, 18.0)]);
    let fp_penalty = if fp_per_kilo < 0.5 { 15.0 } else { 0.0 };
    let we_penalty = if we_per_kilo > 1.0 { 12.0 } else if we_per_kilo > 0.3 { 6.0 } else { 0.0 };
    let concrete_score = if academic_style {
        (concrete_score + fp_penalty + we_penalty).min(45.0)
    } else {
        (concrete_score + fp_penalty + we_penalty).min(95.0)
    } as u32;

    // —— V3 动态统计三维(完全不依赖词表,纯统计 proxy) ——
    let (perplex_cv, uniq_bi, _total_bi) = bigram_freq_cv(text);
    // AI 文本 bigram 频率分布更集中(反复用同样搭配) → cv 高 → 加 AI 分
    // 人类文本词汇分散 → cv 低
    let perplex_score = if uniq_bi < 10 {
        0
    } else if perplex_cv > 0.2 {
        // CV > 0.2:频率明显集中 → AI 信号
        interp(perplex_cv, &[(0.15, 15.0), (0.25, 40.0), (0.4, 65.0), (0.6, 85.0)]) as u32
    } else {
        0
    };

    let ent_cv = sentence_entropy_cv(&sents);
    // AI 各句熵差异小(均匀填充) → cv 低 → 加 AI 分
    let ent_score = if sents.len() < 5 {
        0
    } else if ent_cv < 0.3 {
        interp(ent_cv, &[(0.05, 75.0), (0.15, 55.0), (0.25, 35.0), (0.35, 15.0)]) as u32
    } else {
        0
    };

    let rh_cv = punct_rhythm_cv(text);
    // 中文标点节奏区分度有限,设宽松阈值:AI 间隔略集中 → cv 低
    let rh_score = if rh_cv < 0.45 {
        interp(rh_cv, &[(0.2, 45.0), (0.3, 35.0), (0.4, 25.0), (0.5, 10.0)]) as u32
    } else {
        0
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
            key: "perplexity".into(),
            name: "频率均匀度".into(),
            raw: if uniq_bi < 10 {
                "样本不足".into()
            } else {
                format!("二元组 CV={perplex_cv:.2}(唯一 {uniq_bi})")
            },
            score: perplex_score,
            level: dim_level(perplex_score),
            example: String::new(),
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
            key: "entropy_var".into(),
            name: "句熵变异度".into(),
            raw: if sents.len() < 5 {
                format!("样本不足({} 句)", sents.len())
            } else {
                format!("熵 CV={ent_cv:.2}")
            },
            score: ent_score,
            level: dim_level(ent_score),
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
            key: "punct_rhythm".into(),
            name: "标点节奏".into(),
            raw: format!("节奏 CV={rh_cv:.2}"),
            score: rh_score,
            level: dim_level(rh_score),
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

    // —— 总分:九维加权 + 规则加成 ——
    // 权重重新分配:把词表信号维度权重压低,让纯统计 proxy 说话
    // cliche 0.25 → 0.22, burstiness 0.20 → 0.18, lexical 0.15 → 0.12,
    // connective 0.15 → 0.12, concrete 0.15 → 0.12, template 0.10 → 0.08
    // 新增三维:perplexity 0.10, entropy_var 0.08, punct_rhythm 0.08
    const W: [f64; 9] = [0.22, 0.18, 0.10, 0.12, 0.08, 0.12, 0.12, 0.08, 0.08];
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

    // —— 句子热力标注:先看统计特征,关键词只是佐证 ——
    let sentence_marks = sents
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let l = len_no_ws(s) as f64;
            // 与前后句长差:如果前后都几乎一样,说明是匀速文本
            let prev_len = if i == 0 { l } else { lens[i - 1] };
            let next_len = if i + 1 >= lens.len() { l } else { lens[i + 1] };
            let local_delta = (l - prev_len).abs().min((l - next_len).abs());

            let mut score = 0u32;
            let mut reasons: Vec<String> = Vec::new();

            // 1. 纯统计基底(换同义词也逃不掉)
            if sents.len() >= 5 {
                if (l - mean).abs() < std * 0.5 {
                    score += 30;
                    reasons.push("句长接近均值".into());
                }
                if local_delta < 2.0 {
                    score += 20;
                    reasons.push("与前后句几乎等长".into());
                }
                if local_delta < 1.0 {
                    score += 15;
                }
            }

            // 2. 关键词佐证(会被换同义词绕过,但抓住一个就是一个)
            for w in &lex.cliche {
                if s.contains(w.as_str()) {
                    score += 20;
                    reasons.push(format!("套话「{w}」"));
                    break;
                }
            }
            for w in &lex.hype {
                if s.contains(w.as_str()) {
                    score += 15;
                    reasons.push(format!("黑话「{w}」"));
                    break;
                }
            }
            for w in &lex.connectives {
                if s.contains(w.as_str()) {
                    score += 10;
                    reasons.push(format!("连接词「{w}」"));
                    break;
                }
            }

            // 3. 具体性减分:有数字/引号 → 不是 AI 典型特征
            let has_concrete = s.chars().any(|c| c.is_ascii_digit())
                || s.contains('「')
                || s.contains('《');
            if has_concrete {
                score = score.saturating_sub(15);
                reasons.push("含具体内容".into());
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

// —— 测试 ——

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
    fn keyword_evasion_still_detected() {
        // 用户反馈:规避关键词就完全豁免。
        // 这段 AI 生成文本把所有词表关键词换成同义词/变体,但统计特征仍明显。
        const EVADED: &str = "归纳而言，算法产业的持续进步带来了广泛影响。\
技术是推动文明前进的关键因素。第一，我们得厘清方向。第二，我们得制定方案。\
第三，我们得推进落实。第四，我们得复盘总结。同时，要说明一点，团队配合也很关键。\
因此，我们要加强协作。此外，我们还要完善机制。归纳而言，唯有持续投入，方能取得更好成效。\
整体来看，行业发展的前景十分广阔。不可否认，这条路并不轻松。毋庸置疑，坚持就是胜利。";

        let r = analyze(EVADED);
        // 关键:即使规则命中为 0(换掉了所有词表关键词),总分仍应 ≥ 55
        // —— 空泛 proxy + 句长均匀 + 少具体信号 三个统计基底仍然存在
        assert!(
            r.score >= 55,
            "规避关键词的 AI 文本仍应 ≥55,实际 {} (规则命中 {:?})",
            r.score,
            r.rule_hits.iter().map(|h| h.rule_id).collect::<Vec<_>>()
        );
        // 具体性和句长均匀维度应该偏高(AI 典型特征)
        let concrete = r.dims.iter().find(|d| d.key == "concrete").unwrap();
        assert!(concrete.score >= 55, "规避文本具体性维度应偏高,实际 {}", concrete.score);
        let burst = r.dims.iter().find(|d| d.key == "burstiness").unwrap();
        assert!(burst.score >= 55, "规避文本句长均匀度应偏高,实际 {}", burst.score);
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
                "perplexity",
                "lexical",
                "entropy_var",
                "connective",
                "concrete",
                "punct_rhythm",
                "template"
            ]
        );
        let names: Vec<&str> = r.dims.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "套话密度",
                "句长均匀度",
                "频率均匀度",
                "词汇多样性",
                "句熵变异度",
                "连接词密度",
                "具体性",
                "标点节奏",
                "结构模板度"
            ]
        );
    }

    #[test]
    fn v3_dynamic_dims_discriminate_ai_vs_human() {
        // V3 新增的三个纯统计维度应该方向正确:
        // perplexity: AI bigram 频率集中(CV 高) → 高分;人类分散(CV 低) → 低分
        // entropy_var: AI 各句熵均匀(CV 低) → 高分
        // punct_rhythm: AI 节奏略集中(CV 低) → 加分
        let ai = analyze(AI_SAMPLE);
        let hm = analyze(HUMAN_SAMPLE);

        let ai_perp = ai.dims.iter().find(|d| d.key == "perplexity").unwrap();
        let hm_perp = hm.dims.iter().find(|d| d.key == "perplexity").unwrap();
        assert!(
            ai_perp.score >= 40,
            "AI 样本 perplexity 应 ≥40(实际 {})",
            ai_perp.score
        );
        assert!(
            hm_perp.score <= 15,
            "人类样本 perplexity 应 ≤15(实际 {})",
            hm_perp.score
        );

        let ai_ent = ai.dims.iter().find(|d| d.key == "entropy_var").unwrap();
        assert!(
            ai_ent.score >= 35,
            "AI 样本 entropy_var 应 ≥35(实际 {})",
            ai_ent.score
        );
    }

    #[test]
    fn v3_evasion_dynamic_dims_still_high() {
        // 规避所有词表关键词的 AI 文本,新增的纯统计维度仍应打高分
        const EVADED: &str = "归纳而言，算法产业的持续进步带来了广泛影响。\
技术是推动文明前进的关键因素。第一，我们得厘清方向。第二，我们得制定方案。\
第三，我们得推进落实。第四，我们得复盘总结。同时，要说明一点，团队配合也很关键。\
因此，我们要加强协作。此外，我们还要完善机制。归纳而言，唯有持续投入，方能取得更好成效。\
整体来看，行业发展的前景十分广阔。不可否认，这条路并不轻松。毋庸置疑，坚持就是胜利。";

        let r = analyze(EVADED);
        // 三个新维度(完全不依赖词表)—— perplexity 应该命中(CV 方向已修正)
        let perp = r.dims.iter().find(|d| d.key == "perplexity").unwrap();
        assert!(
            perp.score >= 25,
            "规避文本 perplexity 维度应命中(≥25,实际 {})",
            perp.score
        );
        // 总分也应该保持高位
        assert!(r.score >= 55, "规避文本总分应 ≥55(实际 {})", r.score);
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
