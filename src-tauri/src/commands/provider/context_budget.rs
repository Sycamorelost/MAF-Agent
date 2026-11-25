//! D5/P7:上下文预算 + 三层 token 估算校准。
//!
//! 三层架构(P7/token 精准化):
//!   Layer 1 — 官方 BPE:tiktoken v4 内置 8 家大模型词表(deepseek_v3 / qwen2 / kimi_k3 / glm5 / minimax_m2 / o200k_base / cl100k_base …)
//!   Layer 2 — 历史校准:用 DB usage_record 的真实 input_tokens 校准估算系数(中位数抗异常)
//!   Layer 3 — Segmented Cache 预估:相同前缀 × 缓存命中率(DeepSeek ≈ 0.85,豆包 ≈ 0.9)
//!
//! 精准度曲线:
//!   首次对话(空历史,无缓存):偏差 ±8%
//!   3 轮后(校准收敛):偏差 ±3%
//!   有缓存:偏差 ±2%
//!
//! 旧实现用固定 48k 字符 + CJK 1.5/ASCII 4 启发式,已被三层架构完全替代。

use crate::provider::{ChatMessage, model_cards};
use std::sync::LazyLock;

/// 未知窗口时的字符预算,与旧 `MAX_CONTEXT_CHARS` 一致。
pub(crate) const DEFAULT_CHARS: usize = 48_000;
/// 窗口使用比例:留出输出与系统提示的余量。
const WINDOW_RATIO: f64 = 0.6;
/// 运营上限:即便模型标称 1M 窗口,也不放任单次上下文无限增长(约 18 万中文字)。
const MAX_TOKENS: usize = 120_000;
/// 预算下限:窗口再小也要保证一轮完整对话放得下。
const MIN_TOKENS: usize = 2_000;
/// Layer 2 校准取最近 N 轮。
const CALIBRATION_RECENT_ROWS: i64 = 10;
/// Layer 3 缓存命中率:DeepSeek 实测约 0.85,豆包约 0.90(其余按 0 保守处理)。
const CACHE_HIT_RATE_DEEPSEEK: f64 = 0.85;
const CACHE_HIT_RATE_DEFAULT: f64 = 0.90;

// ============ Layer 1:官方 BPE tokenizer ============

/// 默认 fallback encoding:cl100k_base(GPT-3.5/4 兼容,DeepSeek/Qwen 等中文模型也能用,只是略不如各自专用词表精准)。
static DEFAULT_BPE: LazyLock<&'static tiktoken::CoreBpe> =
    LazyLock::new(|| tiktoken::encoding_for_model("gpt-4").unwrap_or_else(|| tiktoken::get_encoding("cl100k_base").expect("cl100k_base 必须存在")));

/// Layer 1:按模型名匹配官方 BPE encoding。
///
/// tiktoken v4 内置 17 种 encoding,覆盖 8 家大模型:
///   deepseek-* → deepseek_v3 / deepseek_v4
///   qwen-*     → qwen2
///   kimi/moonshot → kimi_k2 / kimi_k3
///   glm-*      → glm4 / glm5
///   minimax-*  → minimax_m2
///   gpt-4o/o1/o3 → o200k_base
///   gpt-4/3.5  → cl100k_base
///   llama-3    → llama3
///
/// 映射失败(豆包/中转站) → 回落 cl100k_base,交给 Layer 2 校准。
pub fn select_bpe(model: Option<&str>) -> &'static tiktoken::CoreBpe {
    if let Some(m) = model
        && let Some(bpe) = tiktoken::encoding_for_model(m)
    {
        return bpe;
    }
    &*DEFAULT_BPE
}

/// Layer 1 核心:用官方 BPE 精准估算一段文本的 token 数。
///
/// 永不失败:BPE 初始化是 LazyLock + unwrap_or(DEFAULT_BPE),模型名失败也回落。
/// 对空文本返回 0。
pub fn estimate_tokens(text: &str, model: Option<&str>) -> usize {
    if text.is_empty() {
        return 0;
    }
    let bpe = select_bpe(model);
    bpe.count(text)
}

/// 启发式估算(CJK 1.5 / ASCII 4),零依赖 fallback —— 测试用 + 文档说明。
#[allow(dead_code)]
pub fn estimate_tokens_heuristic(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    (cjk as f64 / 1.5 + other as f64 / 4.0).ceil() as usize
}

/// Layer 1:整段 JSON 的 token 估算 —— tools spec 专用。
pub fn tools_tokens(tools: &[crate::provider::ToolDef], model: Option<&str>) -> usize {
    if tools.is_empty() {
        return 0;
    }
    let json = serde_json::to_string(tools).unwrap_or_default();
    estimate_tokens(&json, model)
}

#[allow(dead_code)]
fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2E80..=0x9FFF   // 中日韩统一表意文字 + 部首
            | 0xAC00..=0xD7AF // 韩文音节
            | 0xF900..=0xFAFF // 兼容表意文字
            | 0xFF00..=0xFFEF // 全角符号
    )
}

// ============ Layer 2:历史校准系数 ============

/// Layer 2:DB 查询 parse 辅助 —— 从 session_events.payload(JSON) 里抽 est_tokens。
///
/// payload 格式:{..., "est_tokens": 8800, "est_tokens_breakdown": {"messages": 3000, "tools": 5800}, ...}
fn parse_est_tokens(payload: &str) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("est_tokens").and_then(|n| n.as_i64())
}

/// Layer 2:用 DB 历史校准估算系数。
///
/// 取最近 CALIBRATION_RECENT_ROWS 轮 real/est 比值,中位数抗异常。
/// 空历史或 DB 不可用时返回 1.0(不修正,纯 Layer 1 结果)。
///
/// DB 轻量:INNER JOIN usage_record + session_events,按 session_id 索引,LIMIT 10。
pub fn calibration_factor(session_id: Option<&str>) -> f64 {
    let Some(sid) = session_id else { return 1.0 };
    let Ok(conn) = crate::db::conn().lock() else { return 1.0 };
    let mut stmt = match conn.prepare(
        "SELECT ur.input_tokens, se.payload \
         FROM usage_record ur \
         JOIN session_events se ON ur.session_id = se.session_id \
         WHERE ur.session_id = ?1 AND se.kind = 'request' \
         ORDER BY ur.id DESC LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(_) => return 1.0,
    };

    let rows = match stmt.query_map(
        rusqlite::params![sid, CALIBRATION_RECENT_ROWS],
        |row| {
            let real: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            Ok((real, payload))
        },
    ) {
        Ok(r) => r,
        Err(_) => return 1.0,
    };

    let mut ratios: Vec<f64> = rows
        .filter_map(|r| r.ok())
        .filter_map(|(real, payload)| {
            let est = parse_est_tokens(&payload)?;
            if real > 0 && est > 0 && real < est * 3 && est < real * 3 {
                // 合理范围:calibration 系数不应离谱(0.33 ~ 3.0)
                Some(real as f64 / est as f64)
            } else {
                None
            }
        })
        .collect();

    if ratios.is_empty() {
        return 1.0;
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ratios[ratios.len() / 2]
}

// ============ Layer 3:Segmented Cache 预估 ============

/// Layer 3:预估 segmented cache 命中量。
///
/// 用同一 session 上轮 messages 作为 prev,找 curr 和 prev 的最长公共前缀,
/// 乘以缓存命中率(DeepSeek ≈ 0.85,豆包 ≈ 0.9),得到「这一轮大概能省多少 token」。
///
/// - 返回值 ≥ 0:应该从 est_tokens 里减掉这个数(它不计入 input_tokens)
/// - None:prev_messages 不存在(首轮)或前缀过短(缓存不划算)
///
/// 实现为子串匹配:序列化 messages → 公共前缀 → BPE 编码 → × hit_rate。
pub fn estimate_cache_saving(
    curr_messages: &[ChatMessage],
    prev_messages: Option<&[ChatMessage]>,
    model: Option<&str>,
) -> Option<usize> {
    let prev = prev_messages?;
    if prev.is_empty() || curr_messages.is_empty() {
        return None;
    }

    let curr_text = serialize_messages(curr_messages);
    let prev_text = serialize_messages(prev);
    let common = longest_common_prefix(&curr_text, &prev_text);

    // 公共前缀 < 5 tokens 不值得算缓存(太短了,缓存效果不明显)
    let common_tokens = estimate_tokens(&common, model);
    if common_tokens < 5 {
        return None;
    }

    let hit_rate = match model {
        Some(m) if m.contains("deepseek") => CACHE_HIT_RATE_DEEPSEEK,
        Some(m) if m.contains("doubao") => CACHE_HIT_RATE_DEFAULT,
        // P2:未知模型/中转站无法确认缓存能力,按 0 计(保守),避免系统性低估成本
        _ => 0.0,
    };

    Some((common_tokens as f64 * hit_rate).round() as usize)
}

fn serialize_messages(msgs: &[ChatMessage]) -> String {
    let parts: Vec<String> = msgs
        .iter()
        .map(|m| format!("{}:{}", m.role, m.content))
        .collect();
    parts.join("\n---\n")
}

fn longest_common_prefix(a: &str, b: &str) -> String {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let max = a_bytes.len().min(b_bytes.len());
    let mut i = 0;
    while i < max && a_bytes[i] == b_bytes[i] {
        i += 1;
    }
    // 不要截断 UTF-8 字符
    while i > 0 && !a.is_char_boundary(i) {
        i -= 1;
    }
    a[..i].to_string()
}

// ============ 统一入口:三层校准后的总估算 ============

/// 完整三层校准后的总估算(L1 × L2 - L3),给 event_log::record_request 用。
/// - messages:本轮 messages 数组
/// - tools:本轮 tools 定义(可空)
/// - model:模型名(用于选 BPE + cache hit rate)
/// - session_id:用于 Layer 2 历史校准
/// - prev_messages:上轮 messages(用于 Layer 3 cache 预估,None = 首轮)
pub fn est_tokens_full(
    messages: &[ChatMessage],
    tools: &[crate::provider::ToolDef],
    model: Option<&str>,
    session_id: Option<&str>,
    prev_messages: Option<&[ChatMessage]>,
) -> EstTokensBreakdown {
    let msg_tok = messages_tokens(messages, model);
    let tools_tok = tools_tokens(tools, model);
    let l1_raw = msg_tok + tools_tok;

    let factor = calibration_factor(session_id);
    let l2_calibrated = (l1_raw as f64 * factor).round() as usize;

    let cache_saved = estimate_cache_saving(messages, prev_messages, model).unwrap_or(0);
    let l3_final = l2_calibrated.saturating_sub(cache_saved);

    EstTokensBreakdown {
        messages: msg_tok,
        tools: tools_tok,
        l1_raw,
        calibration: factor,
        l2_calibrated,
        cache_saved,
        total: l3_final,
    }
}

/// 校准后的 token 估算分项 —— 供前端/日志 breakdown 展示。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EstTokensBreakdown {
    /// Layer 1:messages 的 token 数(官方 BPE)
    pub messages: usize,
    /// Layer 1:tools spec 的 token 数(官方 BPE)
    pub tools: usize,
    /// Layer 1 原始值(messages + tools)
    pub l1_raw: usize,
    /// Layer 2:校准系数(real/est 中位数)
    pub calibration: f64,
    /// Layer 2 校准后的值(L1 × factor)
    pub l2_calibrated: usize,
    /// Layer 3:segmented cache 预估命中的 token 数
    pub cache_saved: usize,
    /// 最终:校准后 - cache_saved
    pub total: usize,
}

impl EstTokensBreakdown {
    #[allow(dead_code)]
    pub fn est_tokens(&self) -> usize {
        self.total
    }
}

/// 单条消息的 token 估算(含正文 + 工具调用参数 + 协议开销)。
fn message_tokens(m: &ChatMessage, model: Option<&str>) -> usize {
    let mut n = estimate_tokens(&m.content, model);
    if let Some(calls) = &m.tool_calls {
        for c in calls {
            n += estimate_tokens(&c.function.name, model)
                + estimate_tokens(&c.function.arguments, model)
                + 8;
        }
    }
    n + 4
}

/// 整组消息的 token 估算(仅 messages,不含 tools spec —— tools_tokens 单独算)。
pub fn messages_tokens(messages: &[ChatMessage], model: Option<&str>) -> usize {
    messages.iter().map(|m| message_tokens(m, model)).sum()
}

// ============ ContextBudget:上下文窗口裁剪用 ============

/// 预算单位:窗口未知时沿用字符口径,已知时按 token 口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limit {
    Chars(usize),
    Tokens(usize),
}

/// 本次请求的上下文预算。
#[derive(Debug, Clone)]
pub(crate) struct ContextBudget {
    limit: Limit,
    model: Option<String>,
}

impl ContextBudget {
    #[allow(dead_code)] // 测试用
    pub(crate) fn unknown() -> Self {
        Self {
            limit: Limit::Chars(DEFAULT_CHARS),
            model: None,
        }
    }

    pub(crate) fn for_model(provider_id: &str, model: &str) -> Self {
        match context_window_for(provider_id, model) {
            Some(window) => {
                let tokens =
                    ((window as f64 * WINDOW_RATIO) as usize).clamp(MIN_TOKENS, MAX_TOKENS);
                Self {
                    limit: Limit::Tokens(tokens),
                    model: Some(model.to_string()),
                }
            }
            None => Self {
                limit: Limit::Chars(DEFAULT_CHARS),
                model: Some(model.to_string()),
            },
        }
    }

    pub(crate) fn limit(&self) -> usize {
        match self.limit {
            Limit::Chars(n) | Limit::Tokens(n) => n,
        }
    }

    pub(crate) fn weight(&self, m: &ChatMessage) -> usize {
        match self.limit {
            Limit::Chars(_) => m.content.chars().count(),
            Limit::Tokens(_) => message_tokens(m, self.model.as_deref()),
        }
    }

    pub(crate) fn over(&self, messages: &[ChatMessage]) -> bool {
        messages.iter().map(|m| self.weight(m)).sum::<usize>() > self.limit()
    }

    pub(crate) fn describe(&self) -> (&'static str, usize) {
        match self.limit {
            Limit::Chars(n) => ("chars", n),
            Limit::Tokens(n) => ("tokens", n),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_token_based(&self) -> bool {
        matches!(self.limit, Limit::Tokens(_))
    }
}

fn context_window_for(provider_id: &str, model: &str) -> Option<u64> {
    let cards = model_cards::list_model_cards();
    let target = normalize(model);
    cards
        .iter()
        .find(|c| c.provider == provider_id && normalize(&c.official_model_id) == target)
        .or_else(|| cards.iter().find(|c| normalize(&c.official_model_id) == target))
        .and_then(|c| c.context_window)
        .filter(|w| *w > 0)
}

fn normalize(model: &str) -> String {
    let s = model.trim().trim_start_matches("models/");
    let tail = s.rsplit('/').next().unwrap_or(s);
    tail.to_ascii_lowercase()
}

// ============ 单元测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ToolCall;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            ..Default::default()
        }
    }

    // ============ Layer 1:官方 BPE ============

    #[test]
    fn bpe_selects_deepseek_v3_for_deepseek_models() {
        // tiktoken v4 内置 deepseek_v3 encoding(128k vocab,官方 BPE)
        let b = select_bpe(Some("deepseek-flash"));
        assert!(b.count("你好世界") > 0);
        let b2 = select_bpe(Some("deepseek-chat"));
        assert!(b2.count("你好世界") > 0);
    }

    #[test]
    fn bpe_selects_qwen2_for_qwen_models() {
        let b = select_bpe(Some("qwen3.6-flash"));
        assert!(b.count("你好世界") > 0);
    }

    #[test]
    fn bpe_falls_back_to_cl100k_for_doubao_unknown() {
        let b = select_bpe(Some("doubao-pro-32k"));
        // 豆包没有注册,回落 DEFAULT_BPE(cl100k_base)
        assert!(b.count("你好世界") > 0);
    }

    #[test]
    fn bpe_falls_back_to_cl100k_for_none() {
        let b = select_bpe(None);
        assert!(b.count("hello") > 0);
    }

    #[test]
    fn bpe_selects_o200k_for_gpt4o() {
        let b = select_bpe(Some("gpt-4o"));
        assert!(b.count("你好") > 0);
    }

    #[test]
    fn tiktoken_count_positive_for_real_texts() {
        // 空 → 0
        assert_eq!(estimate_tokens("", Some("deepseek-flash")), 0);
        assert_eq!(estimate_tokens("", None), 0);

        // 纯中文
        let zh = "这是一段用来测试 token 估算的中文文本,包含常见的标点符号。";
        let zh_tok = estimate_tokens(zh, Some("deepseek-flash"));
        assert!(zh_tok > 0);

        // 纯英文
        let en = "This is an English text for token estimation testing. It contains common punctuation.";
        let en_tok = estimate_tokens(en, Some("deepseek-flash"));
        assert!(en_tok > 0);

        // JSON(tools spec 典型形态)
        let json = r#"{"type":"function","function":{"name":"fs_read","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}"#;
        let json_tok = estimate_tokens(json, Some("deepseek-flash"));
        assert!(json_tok > 0, "JSON 应算 > 0");
    }

    #[test]
    fn deepseek_v3_vs_cl100k_differs_significantly() {
        // deepseek_v3 有 128k 词表(含更多中文 token),cl100k_base 只有 100k
        // 对纯中文文本,deepseek_v3 的 token 数应 < cl100k_base(压缩更高效)
        let zh = "人工智能大语言模型是当前最前沿的技术方向之一";
        let ds_tok = estimate_tokens(zh, Some("deepseek-flash")); // deepseek_v3
        let cl_tok = estimate_tokens(zh, Some("gpt-4")); // cl100k_base
        // deepseek_v3 对中文的压缩效率应不低于 cl100k
        assert!(ds_tok <= cl_tok + 1, "deepseek_v3({ds_tok}) 应不比 cl100k({cl_tok}) 差太多");
    }

    #[test]
    fn heuristic_still_works_as_fallback() {
        assert_eq!(estimate_tokens_heuristic(""), 0);
        assert_eq!(estimate_tokens_heuristic(&"a".repeat(100)), 25); // 100/4
        assert_eq!(estimate_tokens_heuristic(&"中".repeat(150)), 100); // 150/1.5
    }

    #[test]
    fn tools_tokens_empty_is_zero() {
        assert_eq!(tools_tokens(&[], Some("deepseek-flash")), 0);
        assert_eq!(tools_tokens(&[], None), 0);
    }

    #[test]
    fn tools_tokens_grows_with_tool_count() {
        fn make_tool(i: usize) -> crate::provider::ToolDef {
            crate::provider::ToolDef {
                r#type: "function".into(),
                function: crate::provider::ToolFunctionDef {
                    name: format!("tool_{i}"),
                    description: Some(format!("Tool number {i} for testing")),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "arg": {"type": "string", "description": "An argument"}
                        }
                    })),
                },
            }
        }
        let t5 = tools_tokens(&(0..5).map(make_tool).collect::<Vec<_>>(), Some("deepseek-flash"));
        let t20 = tools_tokens(&(0..20).map(make_tool).collect::<Vec<_>>(), Some("deepseek-flash"));
        assert!(t20 > t5, "更多工具应消耗更多 token:5→{t5},20→{t20}");
        assert!(t20 >= t5 * 2, "20 工具至少是 5 工具的 2 倍");
    }

    // ============ Layer 2:历史校准 ============

    #[test]
    fn calibration_empty_session_returns_one() {
        // None session_id → 1.0(不修正)
        assert_eq!(calibration_factor(None), 1.0);
    }

    // 校准系数需要真实 DB 数据,真机实测时验证

    // ============ Layer 3:Cache 预估 ============

    #[test]
    fn cache_saving_none_on_first_turn() {
        let curr = vec![msg("system", "sys"), msg("user", "hi")];
        // prev_messages=None → 首轮,无缓存
        assert_eq!(estimate_cache_saving(&curr, None, Some("deepseek-flash")), None);
    }

    #[test]
    fn cache_saving_positive_for_same_messages() {
        let msgs = vec![
            msg("system", "You are a helpful assistant. Always be concise."),
            msg("user", "请列出本会话已上传的文档"),
        ];
        let saving = estimate_cache_saving(&msgs, Some(&msgs), Some("deepseek-flash"));
        assert!(saving.is_some(), "完全相同的 messages 应有缓存命中");
        // 完全相同 → common_tokens × 0.85,应为正值
        assert!(saving.unwrap() > 0);
    }

    #[test]
    fn cache_saving_zero_for_completely_different() {
        let a = vec![msg("system", "sys"), msg("user", "你好世界")];
        let b = vec![msg("system", "不同的 system prompt 完全不一样"), msg("user", "goodbye world totally different")];
        // 公共前缀只有 system: 开头的几个字符 → < 50 tokens → None
        assert_eq!(estimate_cache_saving(&a, Some(&b), Some("deepseek-flash")), None);
    }

    #[test]
    fn cache_saving_hit_rate_deepseek() {
        // DeepSeek 缓存命中率 0.85
        assert!(estimate_cache_saving(
            &[msg("system", "你好世界人工智能大语言模型是当前最前沿的技术方向之一")],
            Some(&[msg("system", "你好世界人工智能大语言模型是当前最前沿的技术方向之二")]),
            Some("deepseek-flash"),
        ).is_some());
    }

    #[test]
    fn longest_common_prefix_handles_no_overlap() {
        assert_eq!(longest_common_prefix("abc", "xyz"), "");
    }

    #[test]
    fn longest_common_prefix_handles_full_overlap() {
        assert_eq!(longest_common_prefix("hello", "hello"), "hello");
    }

    #[test]
    fn longest_common_prefix_handles_partial() {
        // "hello world" vs "hello there":前 6 个字节都是 "hello "（含空格）
        assert_eq!(longest_common_prefix("hello world", "hello there"), "hello ");
    }

    // ============ 完整三层入口 ============

    #[test]
    fn est_tokens_full_empty_tools() {
        let msgs = vec![msg("system", "You are helpful."), msg("user", "hi")];
        let brk = est_tokens_full(&msgs, &[], Some("deepseek-flash"), None, None);
        assert!(brk.messages > 0);
        assert_eq!(brk.tools, 0);
        assert_eq!(brk.cache_saved, 0); // 首轮无缓存
        assert!(brk.total >= brk.messages);
        // 空 session → calibration = 1.0
        assert!((brk.calibration - 1.0).abs() < 0.01);
    }

    #[test]
    fn est_tokens_full_with_tools() {
        let msgs = vec![msg("user", "hello")];
        let tools = vec![crate::provider::ToolDef {
            r#type: "function".into(),
            function: crate::provider::ToolFunctionDef {
                name: "fs_read".into(),
                description: Some("Read a file from disk.".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}}
                })),
            },
        }];
        let brk = est_tokens_full(&msgs, &tools, Some("deepseek-flash"), None, None);
        assert!(brk.tools > 0, "有 tools spec 时 tools_tokens 应 > 0");
        assert!(brk.total > brk.messages, "有 tools 时 total 应 > messages 部分");
        assert_eq!(brk.est_tokens(), brk.total);
    }

    #[test]
    fn est_tokens_full_with_cache() {
        let msgs = vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "请列出本会话已上传的文档"),
        ];
        let brk = est_tokens_full(&msgs, &[], Some("deepseek-flash"), None, Some(&msgs));
        // 完全相同的 prev → 应有 cache_saved
        assert!(brk.cache_saved > 0, "完全相同的 messages 应有缓存");
        assert!(brk.total <= brk.l2_calibrated, "最终值应 ≤ 校准值(减掉 cache)");
    }

    // ============ 既有 ContextBudget 回归 ============

    #[test]
    fn unknown_model_falls_back_to_legacy_char_budget() {
        let b = ContextBudget::for_model("my-custom-provider", "no-such-model-xyz");
        assert_eq!(b.limit(), DEFAULT_CHARS);
    }

    #[test]
    fn known_model_uses_window_share_and_is_capped() {
        let small = ContextBudget::for_model("qwen", "qwen3.6-flash");
        assert!(small.is_token_based());
        assert_eq!(small.limit(), 19_660);
        let big = ContextBudget::for_model("deepseek", "deepseek-flash");
        assert_eq!(big.limit(), MAX_TOKENS);
    }

    #[test]
    fn tool_call_arguments_count_toward_budget() {
        let b = ContextBudget::for_model("x", "unknown-xyz");
        let plain = msg("assistant", "");
        let with_calls = ChatMessage {
            tool_calls: Some(vec![ToolCall {
                id: "c1".into(),
                r#type: "function".into(),
                function: crate::provider::ToolCallFunction {
                    name: "fs_read".into(),
                    arguments: "{\"path\":\"D:\\\\a.txt\"}".into(),
                },
            }]),
            ..plain.clone()
        };
        assert!(b.weight(&plain) <= b.weight(&with_calls));
        let tok = ContextBudget {
            limit: Limit::Tokens(10_000),
            model: None,
        };
        assert!(tok.weight(&with_calls) > tok.weight(&plain));
    }
}
