//! 模型卡元数据(迭代 D7.1)。
//!
//! 对应 idea/06-model-templates.md:每张模型卡必填
//! 官方 model id / context_window / max_output / cost_tier / tags / pricing / memory_injection。
//!
//! 数据来源:内置 13 张卡(与 idea/06 对齐)+ 可选用户覆盖文件
//! `data/model_cards.json`(JSON 数组,按 id 覆盖内置或追加新卡)。

use serde::{Deserialize, Serialize};

mod builtin_cards;
mod catalog;
mod user;
mod vendor;

/// 价格信息(刊例价,人民币 / 百万 tokens;生图模型为元/张)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPricing {
    /// 缓存命中输入价
    #[serde(default)]
    pub input_cache_hit: Option<f64>,
    /// 缓存未命中输入价
    #[serde(default)]
    pub input_cache_miss: Option<f64>,
    /// 输入价(无缓存分级时)
    #[serde(default)]
    pub input: Option<f64>,
    /// 输出价
    #[serde(default)]
    pub output: Option<f64>,
    /// 生图单价(元/张)
    #[serde(default)]
    pub per_image: Option<f64>,
    /// 按字符计价(如 DeepL)
    #[serde(default)]
    pub per_char: Option<f64>,
    /// 特殊计费说明
    #[serde(default)]
    pub note: Option<String>,
}

/// 记忆注入策略(与 idea/03 联动)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInjectionSpec {
    /// full / summary / hybrid / none
    pub strategy: String,
    /// 注入模板,{memory_md} 为占位符
    #[serde(default)]
    pub template: Option<String>,
}

/// 模型默认采样参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelDefaults {
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

/// 模型卡(与 idea/06 字段规范对齐)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCard {
    /// metaagent 内部 slug
    pub id: String,
    pub display_name: String,
    /// 对应 ProviderTemplate.id(deepseek / zhipu / qwen / openai / deepl ...),
    /// 无对应模板时填原始 provider 名(anthropic / ollama-sd 等),仅作展示
    pub provider: String,
    /// 官方 API 传参的 model 字段值
    pub official_model_id: String,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output: Option<u64>,
    /// free / budget / standard / premium / ultra
    pub cost_tier: String,
    /// reasoning / thinking / vision / tool / json / long-ctx / chinese / image / translation / local
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pricing: Option<ModelPricing>,
    #[serde(default)]
    pub defaults: Option<ModelDefaults>,
    #[serde(default)]
    pub memory_injection: Option<MemoryInjectionSpec>,
}

#[allow(unused_imports)]
pub use user::{
    USD_TO_CNY, card_is_user_overridden, list_model_cards, save_user_card, validate_model_card,
};
#[allow(unused_imports)]
pub use vendor::{
    FetchedModel, derive_vendor_cards, load_model_cache, model_cache_path, parse_fetched_models,
    save_model_cache, vendor_card_id,
};
