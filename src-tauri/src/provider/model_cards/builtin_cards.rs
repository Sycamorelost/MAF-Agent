//! 内置模型卡硬编码数据(从 model_cards.rs 拆出)。

use super::*;

/// 内置 13 张模型卡(与 idea/06 一一对应)。
pub fn builtin_model_cards() -> Vec<ModelCard> {
    vec![
        // 一、DeepSeek 系列
        ModelCard {
            id: "deepseek-flash".into(),
            display_name: "DeepSeek V4.1 Flash".into(),
            provider: "deepseek".into(),
            official_model_id: "deepseek-flash".into(),
            context_window: Some(1_048_576),
            max_output: Some(393_216),
            cost_tier: "budget".into(),
            tags: vec!["thinking", "vision", "tool", "json", "long-ctx"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input_cache_hit: Some(0.0216),
                input_cache_miss: Some(1.08),
                output: Some(4.32),
                note: Some("非峰值半价;峰值为非峰值 2 倍。兼容 OpenAI / Anthropic 两种格式".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.7),
                top_p: Some(0.95),
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "hybrid".into(),
                template: Some(
                    "<!-- MEMORY_START -->\n以下是本会话的记忆文件(memory.md),你刚接替另一模型继续对话。\n{memory_md}\n请基于它继续,不要重复已覆盖内容。\n<!-- MEMORY_END -->"
                        .into(),
                ),
            }),
        },
        ModelCard {
            id: "deepseek-v4-pro".into(),
            display_name: "DeepSeek V4 Pro".into(),
            provider: "deepseek".into(),
            official_model_id: "deepseek-v4-pro".into(),
            context_window: Some(1_048_576),
            max_output: Some(393_216),
            cost_tier: "premium".into(),
            tags: vec!["thinking", "reasoning", "tool", "json", "long-ctx"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input_cache_hit: Some(0.1584),
                input_cache_miss: Some(4.752),
                output: Some(14.256),
                note: Some("峰值价为刊例价 2 倍;不支持 Vision".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.6),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "hybrid".into(),
                template: Some("<!-- MEMORY_START -->\n{memory_md}\n<!-- MEMORY_END -->".into()),
            }),
        },
        // 二、智谱 GLM 系列
        ModelCard {
            id: "glm-5.3".into(),
            display_name: "GLM-5.3".into(),
            provider: "zhipu".into(),
            official_model_id: "glm-5.3".into(),
            context_window: Some(1_048_576),
            max_output: Some(8192),
            cost_tier: "ultra".into(),
            tags: vec!["reasoning", "tool", "json", "long-ctx", "chinese"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input_cache_hit: Some(2.0),
                input_cache_miss: Some(8.0),
                output: Some(28.0),
                note: Some("缓存存储限时免费".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.7),
                top_p: Some(0.9),
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "summary".into(),
                template: Some(
                    "[会话记忆 · memory.md,刚从其他模型切换]\n{memory_md}\n请直接开始回答,不要重复\"让我看看记忆\"之类的话。"
                        .into(),
                ),
            }),
        },
        ModelCard {
            id: "glm-4-long".into(),
            display_name: "GLM-4-Long".into(),
            provider: "zhipu".into(),
            official_model_id: "glm-4-long".into(),
            context_window: Some(1_048_576),
            max_output: Some(4096),
            cost_tier: "budget".into(),
            tags: vec!["long-ctx", "chinese", "doc-analysis"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input: Some(1.0),
                output: Some(4.0),
                note: Some("长文本/记忆任务专用,价格最低".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.5),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "full".into(),
                template: Some("<!-- MEMORY_FULL_INJECTION -->\n{memory_md}".into()),
            }),
        },
        // 三、阿里百炼千问系列
        ModelCard {
            id: "qwen-3.8-max".into(),
            display_name: "Qwen 3.8 Max".into(),
            provider: "qwen".into(),
            official_model_id: "qwen3.8-max".into(),
            context_window: Some(1_048_576),
            max_output: Some(8192),
            cost_tier: "ultra".into(),
            tags: vec!["reasoning", "vision", "tool", "long-ctx", "chinese"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input: Some(15.0),
                output: Some(60.0),
                note: Some("百炼刊例价估算".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.6),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "hybrid".into(),
                template: Some("<!-- MEMORY_START -->\n{memory_md}\n<!-- MEMORY_END -->".into()),
            }),
        },
        ModelCard {
            id: "qwen-3.6-flash".into(),
            display_name: "Qwen 3.6 Flash".into(),
            provider: "qwen".into(),
            official_model_id: "qwen3.6-flash".into(),
            context_window: Some(32_768),
            max_output: Some(4096),
            cost_tier: "budget".into(),
            tags: vec!["fast", "chinese", "cheap", "streaming"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input: Some(4.8),
                output: Some(28.8),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.8),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "summary".into(),
                template: Some("[精简记忆]\n{memory_md}".into()),
            }),
        },
        // 四、Anthropic Claude(无内置 Provider 模板,仅作模型库展示)
        ModelCard {
            id: "claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            provider: "anthropic".into(),
            official_model_id: "claude-sonnet-4-5".into(),
            context_window: Some(200_000),
            max_output: Some(8192),
            cost_tier: "standard".into(),
            tags: vec!["long-ctx", "vision", "tool", "code"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input: Some(18.0),
                output: Some(72.0),
                note: Some("百炼刊例价".into()),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.6),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: Some(MemoryInjectionSpec {
                strategy: "summary".into(),
                template: Some("以下是记忆摘要(memory.md)。你刚接替另一模型。\n{memory_md}".into()),
            }),
        },
        // 五、生图模型
        ModelCard {
            id: "qwen-image-2.0-pro".into(),
            display_name: "Qwen Image 2.0 Pro".into(),
            provider: "qwen".into(),
            official_model_id: "qwen-image-2.0-pro".into(),
            context_window: None,
            max_output: None,
            cost_tier: "standard".into(),
            tags: vec!["image", "chinese", "high-res"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                per_image: Some(0.5),
                note: Some("百炼刊例价(元/张)".into()),
                ..Default::default()
            }),
            defaults: None,
            memory_injection: None,
        },
        ModelCard {
            id: "wan2.7-image-pro".into(),
            display_name: "Wan 2.7 Image Pro(万相)".into(),
            provider: "qwen".into(),
            official_model_id: "wan2.7-image-pro".into(),
            context_window: None,
            max_output: None,
            cost_tier: "premium".into(),
            tags: vec!["image", "high-quality", "detail"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                per_image: Some(1.5),
                ..Default::default()
            }),
            defaults: None,
            memory_injection: None,
        },
        ModelCard {
            id: "sdxl-turbo-local".into(),
            display_name: "SDXL-Turbo(本地)".into(),
            provider: "ollama-sd".into(),
            official_model_id: "sdxl-turbo".into(),
            context_window: None,
            max_output: None,
            cost_tier: "free".into(),
            tags: vec!["image", "local", "fast", "customizable"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                per_image: Some(0.0),
                note: Some("本地 ComfyUI / 自建推理,无 API 费用".into()),
                ..Default::default()
            }),
            defaults: None,
            memory_injection: None,
        },
        // 六、翻译模型
        ModelCard {
            id: "deepseek-flash-translate".into(),
            display_name: "DeepSeek Flash(翻译模式)".into(),
            provider: "deepseek".into(),
            official_model_id: "deepseek-flash".into(),
            context_window: Some(1_048_576),
            max_output: Some(393_216),
            cost_tier: "budget".into(),
            tags: vec!["translation", "long-text", "fast"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input_cache_hit: Some(0.0216),
                output: Some(4.32),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.1),
                top_p: Some(0.9),
                system_prompt: None,
            }),
            memory_injection: None,
        },
        ModelCard {
            id: "glm-4-long-translate".into(),
            display_name: "GLM-4-Long(翻译模式)".into(),
            provider: "zhipu".into(),
            official_model_id: "glm-4-long".into(),
            context_window: Some(1_048_576),
            max_output: None,
            cost_tier: "budget".into(),
            tags: vec!["translation", "long-document", "chinese"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                input: Some(1.0),
                output: Some(4.0),
                ..Default::default()
            }),
            defaults: Some(ModelDefaults {
                temperature: Some(0.1),
                top_p: None,
                system_prompt: None,
            }),
            memory_injection: None,
        },
        ModelCard {
            id: "deepl-pro".into(),
            display_name: "DeepL Pro".into(),
            provider: "deepl".into(),
            official_model_id: "deepl-pro".into(),
            context_window: None,
            max_output: None,
            cost_tier: "standard".into(),
            tags: vec!["translation", "premium", "multi-lang", "glossary"]
                .into_iter()
                .map(Into::into)
                .collect(),
            pricing: Some(ModelPricing {
                per_char: Some(0.00002),
                note: Some("订阅制按字符计".into()),
                ..Default::default()
            }),
            defaults: None,
            memory_injection: None,
        },
    ]
}

