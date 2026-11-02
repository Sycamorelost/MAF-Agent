//! 检测引擎的版本、免责声明与报告数据结构。

use serde::{Deserialize, Serialize};

use super::super::rules::RuleHit;

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

/// 空文本的九维占位(全部 0,不参与评分)。
pub(crate) fn zero_dims() -> Vec<DimScore> {
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
