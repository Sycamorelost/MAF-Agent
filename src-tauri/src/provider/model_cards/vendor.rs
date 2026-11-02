use serde::{Deserialize, Serialize};

use super::catalog::vendor_catalog;
use super::user::USD_TO_CNY;
use super::{ModelCard, ModelPricing};

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
    pub reasoning: Option<crate::provider::reasoning::ReasoningCapability>,
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
    // FetchedModel 体积远大于 String,装箱避免整个枚举被撑大
    Full(Box<FetchedModel>),
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
                    CachedEntry::Full(f) => *f,
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
        let first_u64 =
            |keys: &[&str]| keys.iter().find_map(|k| m.get(*k).and_then(|v| v.as_u64()));
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
            max_output: first_u64(&["max_output_tokens", "max_completion_tokens", "max_output"]),
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
        let to_cny_m =
            |per_token: f64| (per_token * 1_000_000.0 * USD_TO_CNY * 10_000.0).round() / 10_000.0;
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

/// 按目标 id 列表增删缓存条目:保留既有条目的元数据(上下文/价格/思考修正),
/// 新 id 补仅含 id 的最小条目,顺序以目标列表为准(设置页手动增删/批量导入共用)。
pub fn merge_fetched_models(existing: &[FetchedModel], ids: &[String]) -> Vec<FetchedModel> {
    ids.iter()
        .map(|id| {
            existing
                .iter()
                .find(|m| &m.id == id)
                .cloned()
                .unwrap_or_else(|| FetchedModel::id_only(id.clone()))
        })
        .collect()
}

/// 按模型 id 关键词保守识别图像生成模型(厂家 /models 不返回模态时的兜底,
/// 如火山方舟:seedream 文生图模型的 output_modalities 为空)。
/// 只匹配明确是「生图」的型号关键词,避免把图像理解(vision)模型误列进来。
fn looks_like_image_model(id: &str) -> bool {
    let m = id.to_ascii_lowercase();
    const KEYS: [&str; 9] = [
        "seedream",
        "dall-e",
        "cogview",
        "flux",
        "stable-diffusion",
        "sdxl",
        "t2i",
        "kolors",
        "gpt-image",
    ];
    KEYS.iter().any(|k| m.contains(k))
}

/// 合并图像模型候选:模板预置在前,追加缓存中声明 image 输出模态的模型;
/// 厂家不返回模态时(如火山方舟)按 id 关键词兜底识别 seedream 等生图型号。
pub fn merge_image_models(preset: &[String], fetched: &[FetchedModel]) -> Vec<String> {
    let mut out: Vec<String> = preset.to_vec();
    for m in fetched {
        let is_image =
            m.output_modalities.iter().any(|x| x.contains("image")) || looks_like_image_model(&m.id);
        if is_image && !out.contains(&m.id) {
            out.push(m.id.clone());
        }
    }
    out
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
                context_window: f
                    .context_window
                    .or(cat.as_ref().and_then(|c| c.context_window)),
                max_output: f.max_output.or(cat.as_ref().and_then(|c| c.max_output)),
                cost_tier: cat
                    .map(|c| c.cost_tier.to_string())
                    .unwrap_or_else(|| "standard".into()),
                tags,
                pricing,
                defaults: None,
                memory_injection: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::builtin_cards;
    use super::*;

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
                CachedEntry::Full(f) => f.as_ref().clone(),
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
        let g46 = cards
            .iter()
            .find(|c| c.official_model_id == "glm-4.6")
            .unwrap();
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
    fn merge_fetched_models_preserves_metadata_and_adds_minimal_entries() {
        let existing = vec![
            FetchedModel {
                id: "a".into(),
                name: Some("模型 A".into()),
                context_window: Some(128_000),
                max_output: None,
                input_modalities: vec!["text".into()],
                output_modalities: vec!["text".into()],
                pricing: Some(ModelPricing {
                    input: Some(1.0),
                    output: Some(2.0),
                    ..Default::default()
                }),
                reasoning: Some(crate::provider::reasoning::ReasoningCapability {
                    enabled: true,
                    can_disable: true,
                    levels: true,
                }),
            },
            FetchedModel::id_only("b".into()),
        ];
        // 删除 b、新增 c、保留 a 的元数据;顺序按目标列表
        let merged = merge_fetched_models(&existing, &["a".to_string(), "c".to_string()]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "a");
        assert_eq!(merged[0].context_window, Some(128_000));
        assert_eq!(merged[0].pricing.as_ref().unwrap().input, Some(1.0));
        assert!(merged[0].reasoning.is_some());
        assert_eq!(merged[1].id, "c");
        assert!(merged[1].context_window.is_none());
        assert!(merged[1].pricing.is_none());
    }

    #[test]
    fn merge_image_models_appends_image_output_models() {
        let preset = vec!["dall-e-3".to_string()];
        let image_only = |id: &str, out: &[&str]| FetchedModel {
            id: id.to_string(),
            name: None,
            context_window: None,
            max_output: None,
            input_modalities: vec!["text".into()],
            output_modalities: out.iter().map(|s| s.to_string()).collect(),
            pricing: None,
            reasoning: None,
        };
        let fetched = vec![
            image_only("doubao-seedream-4-0", &["image"]),
            image_only("deepseek-flash", &["text"]),
            // 与预置重复:不重复追加
            image_only("dall-e-3", &["image"]),
        ];
        let merged = merge_image_models(&preset, &fetched);
        assert_eq!(merged, vec!["dall-e-3", "doubao-seedream-4-0"]);
    }

    #[test]
    fn merge_image_models_keyword_fallback_when_modality_missing() {
        // 火山方舟等厂家 /models 不返回 output_modalities:按 id 关键词兜底识别
        let no_modality = |id: &str| FetchedModel {
            id: id.to_string(),
            name: None,
            context_window: None,
            max_output: None,
            input_modalities: Vec::new(),
            output_modalities: Vec::new(),
            pricing: None,
            reasoning: None,
        };
        let fetched = vec![
            no_modality("doubao-seedream-5-0-pro-260628"),
            no_modality("flux-schnell"),
            // 视频生成模型不匹配生图关键词,不应混入
            no_modality("doubao-seedance-1-0-pro-250528"),
            // 普通对话模型
            no_modality("doubao-seed-2-1-lite-260915"),
        ];
        let merged = merge_image_models(&[], &fetched);
        assert_eq!(merged, vec!["doubao-seedream-5-0-pro-260628", "flux-schnell"]);
    }
}
