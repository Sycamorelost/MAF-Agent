use super::ModelCard;
use super::builtin_cards;
use super::vendor::{derive_vendor_cards, load_model_cache};

/// 美元兑人民币估算汇率(仅用于把 USD 刊例换算为元)。
/// 计价快照会记录该值,汇率调整只影响新记录,历史成本不跳变。
pub const USD_TO_CNY: f64 = 7.15;

/// 用户覆盖文件:`<data_dir>/model_cards.json`(JSON 数组)。
fn user_cards_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("model_cards.json")
}

/// 该卡 id 是否被用户文件整体覆盖。
/// 用于区分计价来源:用户手改价记 `custom`,内置/厂家刊例记 `model_card`。
pub fn card_is_user_overridden(id: &str) -> bool {
    let p = user_cards_path();
    if !p.exists() {
        return false;
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<ModelCard>>(&s).ok())
        .map(|cards| cards.iter().any(|c| c.id == id))
        .unwrap_or(false)
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
    if p.exists()
        && let Ok(s) = std::fs::read_to_string(&p)
        && let Ok(user_cards) = serde_json::from_str::<Vec<ModelCard>>(&s)
    {
        for uc in user_cards {
            if let Some(slot) = cards.iter_mut().find(|c| c.id == uc.id) {
                *slot = uc;
            } else {
                cards.push(uc);
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
    if let Some(n) = c.context_window
        && !(1..=1_000_000_000).contains(&n)
    {
        return Err("上下文窗口数值非法".into());
    }
    if let Some(n) = c.max_output
        && !(1..=1_000_000_000).contains(&n)
    {
        return Err("最大输出数值非法".into());
    }
    if c.tags.len() > 32
        || c.tags
            .iter()
            .any(|t| t.len() > 40 || t.chars().any(|ch| ch.is_control()))
    {
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
        if let Some(n) = &p.note
            && (n.len() > 300 || n.chars().any(|ch| ch.is_control()))
        {
            return Err("价格说明非法".into());
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
    use crate::provider::model_cards::ModelPricing;

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
        assert!(
            list_model_cards()
                .iter()
                .any(|c| c.id == "vendor-zhipu-glm-4.7" && c.cost_tier == "premium")
        );

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
}
