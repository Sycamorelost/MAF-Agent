//! 计价上下文与成本计算(纳元口径)。

use crate::db;
use crate::provider::model_cards;

/// 计价上下文:单价(元/百万 tokens)+ 计价快照与来源。
///
/// 三档输入价分开保存(命中 / 未命中 / 未分级),按 `cached_tokens` 分段计价;
/// 无 cached 信息时整段按未命中价,并在快照中标注 `cache: unknown`。
pub(crate) struct PricingCtx {
    input_hit: Option<f64>,
    input_miss: Option<f64>,
    output: Option<f64>,
    /// 生图单价(元/张)
    per_image: Option<f64>,
    /// 按字符计价(元/千字符,DeepL 等)
    per_char: Option<f64>,
    pub(crate) snapshot: String,
    /// model_card | custom
    pub(crate) source: String,
}

/// 查询会话绑定的助手(agent_id, agent_name);未绑定返回 (None, None)。
pub(crate) fn agent_of_session(session_id: &str) -> (Option<String>, Option<String>) {
    let Some(row) = db::sessions::get(session_id).ok().flatten() else {
        return (None, None);
    };
    let Some(aid) = row.agent_id.filter(|s| !s.trim().is_empty()) else {
        return (None, None);
    };
    let name = crate::agents::find(&aid).map(|a| a.name);
    (Some(aid), name)
}

/// 模型 id 规范化:去大小写、`provider/` 前缀、`:free`/`@variant` 后缀与日期后缀。
/// 用于中转站带前缀 / 官方带日期后缀的模型名回退匹配。
fn normalize_model_id(s: &str) -> String {
    let mut m = s.trim().to_ascii_lowercase();
    if let Some(pos) = m.rfind('/') {
        m = m[pos + 1..].to_string();
    }
    if let Some(pos) = m.find(['@', ':']) {
        m.truncate(pos);
    }
    // 反复剥离全数字尾部:20241022 / 2024-08-06 / -0613 / -06
    for _ in 0..5 {
        let Some(pos) = m.rfind('-') else { break };
        let tail = &m[pos + 1..];
        let digits = !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit());
        if digits && matches!(tail.len(), 2 | 4 | 8) {
            m.truncate(pos);
        } else {
            break;
        }
    }
    m
}

/// 匹配模型卡:先精确(provider + official id),再按规范化 id 回退,
/// 最后跨 provider 按规范化 id 匹配(中转站常把模型挂在自定义 provider 下)。
fn find_pricing_card(provider_id: &str, model: &str) -> Option<model_cards::ModelCard> {
    let cards = model_cards::list_model_cards();
    if let Some(c) = cards
        .iter()
        .find(|c| c.provider == provider_id && c.official_model_id == model)
    {
        return Some(c.clone());
    }
    let norm = normalize_model_id(model);
    if let Some(c) = cards
        .iter()
        .find(|c| c.provider == provider_id && normalize_model_id(&c.official_model_id) == norm)
    {
        return Some(c.clone());
    }
    cards
        .iter()
        .find(|c| normalize_model_id(&c.official_model_id) == norm)
        .cloned()
}

/// 查模型卡定价并生成快照;无定价信息返回 None(成本记为未知)。
pub(crate) fn pricing_for(provider_id: &str, model: &str) -> Option<PricingCtx> {
    let card = find_pricing_card(provider_id, model)?;
    let p = card.pricing.as_ref()?;
    if p.input.is_none()
        && p.input_cache_hit.is_none()
        && p.input_cache_miss.is_none()
        && p.output.is_none()
        && p.per_image.is_none()
        && p.per_char.is_none()
    {
        return None;
    }
    let hit = p.input_cache_hit.or(p.input);
    let miss = p.input_cache_miss.or(p.input_cache_hit).or(p.input);
    let source = if model_cards::card_is_user_overridden(&card.id) {
        "custom"
    } else {
        "model_card"
    };
    let snapshot = serde_json::json!({
        "input_hit": hit,
        "input_miss": miss,
        "output": p.output,
        "per_image": p.per_image,
        "per_char": p.per_char,
        "currency": "CNY",
        "unit": "per_million_tokens",
        "fx_rate": model_cards::USD_TO_CNY,
        "card_id": card.id,
        "manual": source == "custom",
        "note": p.note,
        "cache": if p.input_cache_hit.is_some() { "segmented" } else { "unknown" },
    })
    .to_string();
    Some(PricingCtx {
        input_hit: hit,
        input_miss: miss,
        output: p.output,
        per_image: p.per_image,
        per_char: p.per_char,
        snapshot,
        source: source.into(),
    })
}

/// 成本(纳元,1e-9 元):tokens × 单价(元/百万 tokens) = 纳元。
///
/// 口径:`(input - cached) × miss + cached × hit + output × output`。
/// 厂商只回 total_tokens(input/output 均缺失)时无法拆分 → 返回 None(未计价),
/// 不编造单价,避免"标了 model_card 却是 ¥0"的误导。
pub(crate) fn cost_nano_of(
    p: &PricingCtx,
    input: Option<u32>,
    output: Option<u32>,
    cached: Option<u32>,
) -> Option<i64> {
    if input.is_none() && output.is_none() {
        return None;
    }
    price_tokens_nano(
        p,
        input.unwrap_or(0) as i64,
        output.unwrap_or(0) as i64,
        cached.unwrap_or(0) as i64,
    )
}

/// token 口径计价核心(支持聚合后的 i64 合计,用于历史复算)。
pub(crate) fn price_tokens_nano(
    p: &PricingCtx,
    input: i64,
    output: i64,
    cached: i64,
) -> Option<i64> {
    if p.input_hit.is_none() && p.input_miss.is_none() && p.output.is_none() {
        return None;
    }
    let hit_tokens = cached.clamp(0, input.max(0));
    let miss_tokens = input.max(0) - hit_tokens;
    // 命中/未命中价缺失时互相回退,都没有则该段按 0 计
    let miss_price = p.input_miss.or(p.input_hit).unwrap_or(0.0);
    let hit_price = p.input_hit.or(p.input_miss).unwrap_or(miss_price);
    let output_price = p.output.unwrap_or(0.0);
    let yuan = (miss_tokens as f64 * miss_price
        + hit_tokens as f64 * hit_price
        + output.max(0) as f64 * output_price)
        / 1_000_000.0;
    Some((yuan * 1e9).round() as i64)
}

/// 按件计价(生图 / 按字符):images × 元/张 + chars × 元/千字符。
/// 与 token 口径互斥;当前用于绘图落库,避免生图成本恒为 0。
#[allow(dead_code)]
pub(crate) fn cost_nano_units(p: &PricingCtx, images: u32, chars: u32) -> Option<i64> {
    if images == 0 && chars == 0 {
        return None;
    }
    let image_yuan = images as f64 * p.per_image.unwrap_or(0.0);
    let char_yuan = chars as f64 * p.per_char.unwrap_or(0.0) / 1000.0;
    if p.per_image.is_none() && p.per_char.is_none() {
        return None;
    }
    Some(((image_yuan + char_yuan) * 1e9).round() as i64)
}
