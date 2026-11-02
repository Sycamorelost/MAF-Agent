//! 检测主入口:对文本运行本地统计,产出结构化报告。

use super::super::lexicon::{self, count_occ};
use super::super::rules;
use super::stats::*;
use super::types::{
    DimScore, EngineStats, FlavorReport, SentenceMark, DISCLAIMER, ENGINE_VERSION, zero_dims,
};

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
