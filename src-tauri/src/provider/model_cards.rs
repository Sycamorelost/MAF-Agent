//! 模型卡元数据(迭代 D7.1)。
//!
//! 对应 idea/06-model-templates.md:每张模型卡必填
//! 官方 model id / context_window / max_output / cost_tier / tags / pricing / memory_injection。
//!
//! 数据来源:内置 13 张卡(与 idea/06 对齐)+ 可选用户覆盖文件
//! `data/model_cards.json`(JSON 数组,按 id 覆盖内置或追加新卡)。

use serde::{Deserialize, Serialize};

mod builtin_cards;

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

// ---------- 厂家模型拉取:元数据缓存 + 价格补全 ----------

/// 厂家 `/models` 单个模型随拉取一起带回的元数据。
/// 多数厂家只给 id;DeepSeek 等会给上下文/输出上限/模态;价格字段仅部分中转站提供。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedModel {
    /// 官方 model id(请求传参值)
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output: Option<u64>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
    /// 厂家自报价格(无则由内置刊例目录/用户手填补齐)
    #[serde(default)]
    pub pricing: Option<ModelPricing>,
    /// 思考能力声明(v1.9.0 工作块 I;仅由模型卡手动修正写入,厂家 /models 不返回该字段。
    /// 缺省 None = 走内置规则表推断,见 provider/reasoning.rs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<super::reasoning::ReasoningCapability>,
}

impl FetchedModel {
    fn id_only(id: String) -> Self {
        Self {
            id,
            name: None,
            context_window: None,
            max_output: None,
            input_modalities: Vec::new(),
            output_modalities: Vec::new(),
            pricing: None,
            reasoning: None,
        }
    }
}

/// 旧缓存(`Vec<String>`,仅 id)与新缓存(`Vec<FetchedModel>`)的兼容反序列化形态。
#[derive(Deserialize)]
#[serde(untagged)]
enum CachedEntry {
    Id(String),
    Full(FetchedModel),
}

/// 厂家模型缓存文件:`<data_dir>/provider_models.json`,
/// 结构 `{provider_id: [FetchedModel]}`(旧版为字符串数组,读取时自动升级)。
pub fn model_cache_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("provider_models.json")
}

/// 读取厂家模型缓存;旧版纯 id 数组自动升级为仅含 id 的 FetchedModel。
pub fn load_model_cache() -> std::collections::HashMap<String, Vec<FetchedModel>> {
    let p = model_cache_path();
    if !p.exists() {
        return Default::default();
    }
    let Ok(s) = std::fs::read_to_string(&p) else {
        return Default::default();
    };
    let Ok(raw) = serde_json::from_str::<std::collections::HashMap<String, Vec<CachedEntry>>>(&s)
    else {
        return Default::default();
    };
    raw.into_iter()
        .map(|(k, list)| {
            let models = list
                .into_iter()
                .map(|e| match e {
                    CachedEntry::Id(s) => FetchedModel::id_only(s),
                    CachedEntry::Full(f) => f,
                })
                .collect();
            (k, models)
        })
        .collect()
}

/// 保存厂家模型缓存(全量 pretty JSON)。
pub fn save_model_cache(
    cache: &std::collections::HashMap<String, Vec<FetchedModel>>,
) -> Result<(), String> {
    let p = model_cache_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write provider_models.json failed: {e}"))
}

/// 从厂家 `/models` 响应解析全量元数据(而不仅是 id)。
/// 字段名兼容多家变体:context_window/context_length、max_output_tokens/max_completion_tokens。
pub fn parse_fetched_models(json: &serde_json::Value) -> Vec<FetchedModel> {
    let Some(arr) = json.get("data").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in arr {
        let Some(id) = m
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
        else {
            continue;
        };
        let first_u64 = |keys: &[&str]| {
            keys.iter()
                .find_map(|k| m.get(*k).and_then(|v| v.as_u64()))
        };
        let modalities = |key: &str| {
            m.get(key)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        out.push(FetchedModel {
            id,
            name: m
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            context_window: first_u64(&[
                "context_window",
                "context_length",
                "max_context_length",
                "max_context_tokens",
            ]),
            max_output: first_u64(&[
                "max_output_tokens",
                "max_completion_tokens",
                "max_output",
            ]),
            input_modalities: modalities("input_modalities"),
            output_modalities: modalities("output_modalities"),
            pricing: parse_vendor_pricing(m.get("pricing")),
            reasoning: None,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// 解析厂家自报价格,兼容两种常见形态:
/// - 国产 OpenAI 兼容/中转站:`{input, output, input_cache_hit, input_cache_miss}`(元/百万 tokens)
/// - OpenRouter:`{prompt, completion, input_cache_reading}`(USD/token,字符串数字)
///   → 乘 1e6 并按固定汇率估算为元/百万,note 标注估算来源
fn parse_vendor_pricing(v: Option<&serde_json::Value>) -> Option<ModelPricing> {
    let p = v?;
    let get_f = |key: &str| {
        p.get(key)
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
            })
            .filter(|x| x.is_finite() && *x > 0.0)
    };
    // 形态一:已是元/百万 tokens
    if let (Some(i), Some(o)) = (get_f("input"), get_f("output")) {
        return Some(ModelPricing {
            input: Some(i),
            output: Some(o),
            input_cache_hit: get_f("input_cache_hit"),
            input_cache_miss: get_f("input_cache_miss"),
            note: p.get("note").and_then(|v| v.as_str()).map(String::from),
            ..Default::default()
        });
    }
    // 形态二:OpenRouter USD/token
    if let (Some(pt), Some(cp)) = (get_f("prompt"), get_f("completion")) {
        /// 美元兑人民币估算汇率(仅用于把 OpenRouter 刊例换算为元/百万展示)
        const USD_TO_CNY: f64 = 7.15;
        let to_cny_m = |per_token: f64| (per_token * 1_000_000.0 * USD_TO_CNY * 10_000.0).round() / 10_000.0;
        return Some(ModelPricing {
            input: Some(to_cny_m(pt)),
            output: Some(to_cny_m(cp)),
            input_cache_hit: get_f("input_cache_reading").map(to_cny_m),
            note: Some(format!(
                "OpenRouter USD 刊例按 {USD_TO_CNY} 汇率估算(元/百万 tokens)"
            )),
            ..Default::default()
        });
    }
    None
}

/// 内置刊例目录的单条补全信息(厂家接口不给价时兜底)。
struct CatalogMeta {
    display_name: &'static str,
    context_window: Option<u64>,
    max_output: Option<u64>,
    cost_tier: &'static str,
    tags: &'static [&'static str],
    input: Option<f64>,
    output: Option<f64>,
    cache_hit: Option<f64>,
    note: Option<&'static str>,
}

/// 已知公开刊例价目录(元/百万 tokens),按 (provider, 官方 model id) 匹配。
/// 仅作拉取补全;任何条目都可被用户手动编辑覆盖。
fn vendor_catalog(provider: &str, model: &str) -> Option<CatalogMeta> {
    if provider != "zhipu" {
        return None;
    }
    Some(match model {
        // 智谱刊例:https://docs.bigmodel.cn/cn/guide/start/pricing(分级计价取入门档,note 标注)
        "glm-4.5" => CatalogMeta {
            display_name: "GLM-4.5",
            context_window: Some(131_072),
            max_output: None,
            cost_tier: "budget",
            tags: &["thinking", "tool", "json", "chinese"],
            input: Some(0.8),
            output: Some(2.0),
            cache_hit: Some(0.1),
            note: Some("智谱刊例价(入门档),长上下文加价,以官网为准"),
        },
        "glm-4.5-air" => CatalogMeta {
            display_name: "GLM-4.5-Air",
            context_window: Some(131_072),
            max_output: None,
            cost_tier: "budget",
            tags: &["tool", "json", "chinese", "fast"],
            input: Some(0.8),
            output: Some(2.0),
            cache_hit: Some(0.16),
            note: Some("智谱刊例价(输出/长上下文分级),以官网为准"),
        },
        "glm-4.6" => CatalogMeta {
            display_name: "GLM-4.6",
            context_window: Some(204_800),
            max_output: Some(131_072),
            cost_tier: "premium",
            tags: &["thinking", "tool", "json", "long-ctx", "chinese"],
            input: Some(3.0),
            output: Some(14.0),
            cache_hit: Some(0.6),
            note: Some("智谱刊例价(≤32K 档),以官网为准"),
        },
        "glm-4.7" => CatalogMeta {
            display_name: "GLM-4.7",
            context_window: Some(204_800),
            max_output: None,
            cost_tier: "standard",
            tags: &["tool", "json", "long-ctx", "chinese"],
            input: Some(2.0),
            output: Some(8.0),
            cache_hit: Some(0.4),
            note: Some("智谱刊例价(入门档,输出/长度分级),以官网为准"),
        },
        "glm-5" => CatalogMeta {
            display_name: "GLM-5",
            context_window: None,
            max_output: None,
            cost_tier: "standard",
            tags: &["reasoning", "tool", "json", "chinese"],
            input: Some(4.0),
            output: Some(18.0),
            cache_hit: Some(1.0),
            note: Some("智谱刊例价(≤32K 档),以官网为准"),
        },
        "glm-5-turbo" => CatalogMeta {
            display_name: "GLM-5-Turbo",
            context_window: None,
            max_output: None,
            cost_tier: "standard",
            tags: &["tool", "json", "chinese", "fast"],
            input: Some(5.0),
            output: Some(22.0),
            cache_hit: Some(1.2),
            note: Some("智谱刊例价(≤32K 档),以官网为准"),
        },
        "glm-5.1" => CatalogMeta {
            display_name: "GLM-5.1",
            context_window: None,
            max_output: None,
            cost_tier: "standard",
            tags: &["reasoning", "tool", "json", "chinese"],
            input: Some(6.0),
            output: Some(24.0),
            cache_hit: Some(1.3),
            note: Some("智谱刊例价(≤32K 档),以官网为准"),
        },
        "glm-5.2" => CatalogMeta {
            display_name: "GLM-5.2",
            context_window: Some(1_048_576),
            max_output: None,
            cost_tier: "premium",
            tags: &["reasoning", "tool", "json", "long-ctx", "chinese"],
            input: Some(8.0),
            output: Some(28.0),
            cache_hit: Some(2.0),
            note: Some("智谱刊例价,以官网为准"),
        },
        "glm-5.3-flash" => CatalogMeta {
            display_name: "GLM-5.3-Flash",
            context_window: Some(1_048_576),
            max_output: None,
            cost_tier: "budget",
            tags: &["vision", "tool", "json", "long-ctx", "chinese", "fast"],
            input: Some(0.8),
            output: Some(2.8),
            cache_hit: Some(0.23),
            note: Some("智谱刊例价,支持图片/视频/文件输入,以官网为准"),
        },
        "glm-5.3-flashx" => CatalogMeta {
            display_name: "GLM-5.3-FlashX",
            context_window: Some(1_048_576),
            max_output: None,
            cost_tier: "standard",
            tags: &["vision", "tool", "json", "long-ctx", "chinese"],
            input: Some(2.0),
            output: Some(7.0),
            cache_hit: Some(0.57),
            note: Some("智谱刊例价,支持图片/视频/文件输入,以官网为准"),
        },
        _ => return None,
    })
}

/// 厂家卡 id:`vendor-{provider}-{official_id 安全化}`,稳定且与内置/用户卡命名空间隔离。
pub fn vendor_card_id(provider: &str, official_model_id: &str) -> String {
    let safe: String = official_model_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("vendor-{provider}-{safe}")
}

/// 依据拉取缓存派生模型卡:厂家自报字段优先,缺口用内置刊例目录补全;
/// 与内置卡(provider + official_model_id)重合的模型不重复生成(内置卡优先)。
pub fn derive_vendor_cards(
    cache: &std::collections::HashMap<String, Vec<FetchedModel>>,
    builtins: &[ModelCard],
) -> Vec<ModelCard> {
    let mut out = Vec::new();
    for (provider, models) in cache {
        for f in models {
            if builtins
                .iter()
                .any(|c| &c.provider == provider && c.official_model_id == f.id)
            {
                continue;
            }
            let cat = vendor_catalog(provider, &f.id);

            // 模态派生标签,与目录标签合并去重(目录在前)
            let mut tags: Vec<String> = Vec::new();
            if let Some(c) = &cat {
                for t in c.tags {
                    tags.push((*t).into());
                }
            }
            if f.input_modalities.iter().any(|m| m.contains("image"))
                && !tags.iter().any(|t| t == "vision")
            {
                tags.push("vision".into());
            }
            if f.output_modalities.iter().any(|m| m.contains("image"))
                && !tags.iter().any(|t| t == "image")
            {
                tags.push("image".into());
            }

            let pricing = f.pricing.clone().or_else(|| {
                cat.as_ref().and_then(|c| {
                    let (Some(i), Some(o)) = (c.input, c.output) else {
                        return None;
                    };
                    Some(ModelPricing {
                        input: Some(i),
                        output: Some(o),
                        input_cache_hit: c.cache_hit,
                        note: c.note.map(String::from),
                        ..Default::default()
                    })
                })
            });

            let display_name = f
                .name
                .clone()
                .or_else(|| cat.as_ref().map(|c| c.display_name.to_string()))
                .unwrap_or_else(|| f.id.clone());

            out.push(ModelCard {
                id: vendor_card_id(provider, &f.id),
                display_name,
                provider: provider.clone(),
                official_model_id: f.id.clone(),
                context_window: f.context_window.or(cat.as_ref().and_then(|c| c.context_window)),
                max_output: f.max_output.or(cat.as_ref().and_then(|c| c.max_output)),
                cost_tier: cat.map(|c| c.cost_tier.to_string()).unwrap_or_else(|| "standard".into()),
                tags,
                pricing,
                defaults: None,
                memory_injection: None,
            });
        }
    }
    out
}



/// 用户覆盖文件:`<data_dir>/model_cards.json`(JSON 数组)。
fn user_cards_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("model_cards.json")
}

/// 模型卡数据分三层,按优先级合并:
/// 1. 内置 13 张(与 idea/06 对齐)
/// 2. 厂家拉取派生卡(data/provider_models.json 的元数据 + 内置刊例目录补价)
/// 3. 用户手填卡 data/model_cards.json(按 id 整体覆盖内置卡或厂家卡)
pub fn list_model_cards() -> Vec<ModelCard> {
    let mut cards = builtin_cards::builtin_model_cards();
    let vendor = derive_vendor_cards(&load_model_cache(), &cards);
    cards.extend(vendor);
    let p = user_cards_path();
    if p.exists() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(user_cards) = serde_json::from_str::<Vec<ModelCard>>(&s) {
                for uc in user_cards {
                    if let Some(slot) = cards.iter_mut().find(|c| c.id == uc.id) {
                        *slot = uc;
                    } else {
                        cards.push(uc);
                    }
                }
            }
        }
    }
    cards
}

/// 校验前端保存的模型卡:id/官方 id/显示名/档位/价格数值的合法性与安全约束。
pub fn validate_model_card(c: &ModelCard) -> Result<(), String> {
    // provider 复用统一 id 白名单(防遍历)
    crate::memory::validate_id(&c.provider)?;
    let valid_id = |s: &str| {
        !s.is_empty()
            && s.len() <= 96
            && !s.contains("..")
            && s.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    };
    if !valid_id(&c.id) {
        return Err("模型卡 id 非法".into());
    }
    let official = c.official_model_id.trim();
    if official.is_empty() || official.len() > 200 {
        return Err("官方模型 ID 非法".into());
    }
    if official.chars().any(|ch| ch.is_control()) {
        return Err("官方模型 ID 含非法控制字符".into());
    }
    let name = c.display_name.trim();
    if name.is_empty() || name.len() > 100 || name.chars().any(|ch| ch.is_control()) {
        return Err("显示名非法".into());
    }
    if !matches!(
        c.cost_tier.as_str(),
        "free" | "budget" | "standard" | "premium" | "ultra"
    ) {
        return Err("成本档位非法".into());
    }
    if let Some(n) = c.context_window {
        if !(1..=1_000_000_000).contains(&n) {
            return Err("上下文窗口数值非法".into());
        }
    }
    if let Some(n) = c.max_output {
        if !(1..=1_000_000_000).contains(&n) {
            return Err("最大输出数值非法".into());
        }
    }
    if c.tags.len() > 32 || c.tags.iter().any(|t| t.len() > 40 || t.chars().any(|ch| ch.is_control())) {
        return Err("标签非法".into());
    }
    if let Some(p) = &c.pricing {
        let prices = [
            p.input,
            p.output,
            p.input_cache_hit,
            p.input_cache_miss,
            p.per_image,
            p.per_char,
        ];
        for x in prices.into_iter().flatten() {
            if !x.is_finite() || !(0.0..=1_000_000.0).contains(&x) {
                return Err("价格数值非法".into());
            }
        }
        if let Some(n) = &p.note {
            if n.len() > 300 || n.chars().any(|ch| ch.is_control()) {
                return Err("价格说明非法".into());
            }
        }
    }
    Ok(())
}

/// 用户手动新增/覆盖一张模型卡,整体写入 data/model_cards.json(按 id upsert)。
pub fn save_user_card(card: &ModelCard) -> Result<(), String> {
    validate_model_card(card)?;
    let p = user_cards_path();
    let mut list: Vec<ModelCard> = if p.exists() {
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    match list.iter_mut().find(|c| c.id == card.id) {
        Some(slot) => *slot = card.clone(),
        None => list.push(card.clone()),
    }
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write model_cards.json failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_13_unique_cards() {
        let cards = builtin_cards::builtin_model_cards();
        assert_eq!(cards.len(), 13);
        let mut ids = cards.iter().map(|c| &c.id).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 13);
    }

    #[test]
    fn cards_have_required_fields() {
        for c in builtin_cards::builtin_model_cards() {
            assert!(!c.display_name.is_empty());
            assert!(!c.official_model_id.is_empty());
            assert!(!c.cost_tier.is_empty());
            assert!(matches!(
                c.cost_tier.as_str(),
                "free" | "budget" | "standard" | "premium" | "ultra"
            ));
            // 推理类模型必须有上下文窗口;生图/DeepL 除外
            if !c.tags.iter().any(|t| t == "image") && c.provider != "deepl" {
                assert!(c.context_window.is_some(), "{} 缺 context_window", c.id);
            }
        }
    }

    #[test]
    fn legacy_string_cache_upgrades_to_fetched_models() {
        let legacy = serde_json::json!({
            "zhipu": ["glm-4.5", "glm-4.6"]
        });
        // 直接验证兼容入口的反序列化形态
        let raw: std::collections::HashMap<String, Vec<CachedEntry>> =
            serde_json::from_value(legacy).unwrap();
        let models: Vec<FetchedModel> = raw
            .get("zhipu")
            .unwrap()
            .iter()
            .map(|e| match e {
                CachedEntry::Id(s) => FetchedModel::id_only(s.clone()),
                CachedEntry::Full(f) => f.clone(),
            })
            .collect();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "glm-4.5");
        assert!(models[0].context_window.is_none());
    }

    #[test]
    fn parse_fetched_captures_metadata_modalities_and_pricing() {
        let json = serde_json::json!({
            "data": [
                {
                    "id": "deepseek-flash",
                    "name": "DeepSeek-V4.1-Flash",
                    "context_window": 1048576,
                    "max_output_tokens": 393216,
                    "input_modalities": ["text", "image"],
                    "output_modalities": ["text"]
                },
                {
                    "id": "relay-x",
                    "context_length": 128000,
                    "max_completion_tokens": 4096,
                    "pricing": {"input": 2.5, "output": 9.0, "input_cache_hit": 0.5}
                },
                {"id": "openrouter/owl", "pricing": {"prompt": "0.000003", "completion": "0.000012"}}
            ]
        });
        let models = parse_fetched_models(&json);
        assert_eq!(models.len(), 3);
        let ds = &models[0];
        assert_eq!(ds.id, "deepseek-flash");
        assert_eq!(ds.context_window, Some(1_048_576));
        assert_eq!(ds.max_output, Some(393_216));
        assert!(ds.input_modalities.contains(&"image".to_string()));
        assert!(ds.pricing.is_none());
        let rx = &models[2];
        assert_eq!(rx.id, "relay-x");
        assert_eq!(rx.context_window, Some(128_000));
        let p = rx.pricing.as_ref().unwrap();
        assert_eq!(p.input, Some(2.5));
        assert_eq!(p.output, Some(9.0));
        let or = &models[1];
        let op = or.pricing.as_ref().unwrap();
        // 0.000003 USD/token × 1e6 × 7.15 = 21.45 元/百万
        assert!((op.input.unwrap() - 21.45).abs() < 1e-6);
        assert!(op.note.as_ref().unwrap().contains("OpenRouter"));
    }

    #[test]
    fn vendor_cards_skip_builtin_and_fill_catalog() {
        let builtins = builtin_cards::builtin_model_cards();
        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "deepseek".to_string(),
            vec![
                // 与内置卡重合 → 跳过
                FetchedModel::id_only("deepseek-flash".into()),
                // 厂家新模型,无目录无价格
                FetchedModel::id_only("deepseek-newbie".into()),
            ],
        );
        cache.insert(
            "zhipu".to_string(),
            vec![
                // 仅 id,目录补全价格/上下文
                FetchedModel::id_only("glm-4.6".into()),
                // 厂家自报元数据 + 模态,目录补价
                FetchedModel {
                    id: "glm-5.3-flash".into(),
                    name: None,
                    context_window: None,
                    max_output: None,
                    input_modalities: vec!["text".into(), "image".into()],
                    output_modalities: vec!["text".into()],
                    pricing: None,
                    reasoning: None,
                },
            ],
        );
        let cards = derive_vendor_cards(&cache, &builtins);
        // 3 张:deepseek-newbie / glm-4.6 / glm-5.3-flash(deepseek-flash 被内置覆盖)
        assert_eq!(cards.len(), 3);
        let newbie = cards
            .iter()
            .find(|c| c.official_model_id == "deepseek-newbie")
            .unwrap();
        assert_eq!(newbie.id, "vendor-deepseek-deepseek-newbie");
        assert!(newbie.pricing.is_none());
        assert_eq!(newbie.cost_tier, "standard");
        let g46 = cards.iter().find(|c| c.official_model_id == "glm-4.6").unwrap();
        assert_eq!(g46.context_window, Some(204_800));
        assert_eq!(g46.pricing.as_ref().unwrap().input, Some(3.0));
        assert_eq!(g46.pricing.as_ref().unwrap().output, Some(14.0));
        let flash = cards
            .iter()
            .find(|c| c.official_model_id == "glm-5.3-flash")
            .unwrap();
        assert!(flash.tags.contains(&"vision".to_string()));
        assert_eq!(flash.pricing.as_ref().unwrap().output, Some(2.8));
    }

    #[test]
    fn vendor_pricing_from_api_overrides_catalog() {
        let builtins = builtin_cards::builtin_model_cards();
        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "zhipu".to_string(),
            vec![FetchedModel {
                id: "glm-4.7".into(),
                name: None,
                context_window: Some(999_999),
                max_output: None,
                input_modalities: vec![],
                output_modalities: vec![],
                pricing: Some(ModelPricing {
                    input: Some(10.0),
                    output: Some(30.0),
                    ..Default::default()
                }),
                reasoning: None,
            }],
        );
        let cards = derive_vendor_cards(&cache, &builtins);
        let c = &cards[0];
        // 厂家自报优先
        assert_eq!(c.context_window, Some(999_999));
        assert_eq!(c.pricing.as_ref().unwrap().input, Some(10.0));
    }

    #[test]
    fn saved_user_card_validates_and_overrides() {
        let mut card = ModelCard {
            id: "vendor-zhipu-glm-4.7".into(),
            display_name: "GLM-4.7 自定义".into(),
            provider: "zhipu".into(),
            official_model_id: "glm-4.7".into(),
            context_window: Some(204_800),
            max_output: None,
            cost_tier: "premium".into(),
            tags: vec![],
            pricing: Some(ModelPricing {
                input: Some(2.5),
                output: Some(8.0),
                ..Default::default()
            }),
            defaults: None,
            memory_injection: None,
        };
        save_user_card(&card).unwrap();
        assert!(list_model_cards()
            .iter()
            .any(|c| c.id == "vendor-zhipu-glm-4.7" && c.cost_tier == "premium"));

        // 非法卡片必须被拒绝
        card.provider = "../evil".into();
        assert!(validate_model_card(&card).is_err());
        card.provider = "zhipu".into();
        card.id = "bad id!".into();
        assert!(validate_model_card(&card).is_err());
        card.id = "ok-id".into();
        card.pricing = Some(ModelPricing {
            input: Some(-1.0),
            ..Default::default()
        });
        assert!(validate_model_card(&card).is_err());

        // 清理测试条目
        let p = user_cards_path();
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(mut list) = serde_json::from_str::<Vec<ModelCard>>(&s) {
                list.retain(|c| c.id != "vendor-zhipu-glm-4.7");
                let _ = std::fs::write(&p, serde_json::to_string_pretty(&list).unwrap());
            }
        }
    }

    #[test]
    fn reasoning_cards_have_memory_injection() {
        let cards = builtin_cards::builtin_model_cards();
        let reasoning = cards
            .iter()
            .filter(|c| {
                !c.tags.iter().any(|t| t == "image" || t == "translation")
            })
            .count();
        let with_injection = cards
            .iter()
            .filter(|c| c.memory_injection.is_some())
            .count();
        assert_eq!(reasoning, with_injection);
    }
}
