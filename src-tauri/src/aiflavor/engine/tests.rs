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
