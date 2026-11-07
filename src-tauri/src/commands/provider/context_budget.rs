//! D5:按模型上下文窗口计算本次请求的上下文预算。
//!
//! 旧实现用固定 48k 字符:小窗口模型仍可能溢出,大窗口模型又过早压缩。
//! 这里以模型卡的 `context_window` × 60% 作为 token 预算,并按文本构成估算 token
//! (CJK ≈ 1.5 字符/token,其余 ≈ 4 字符/token)。查不到窗口时回落到旧的 48k 字符,
//! 不改变既有行为。
//!
//! 纯计算模块:不做 IO 之外的裁剪,裁剪顺序由 compress.rs / chat.rs 决定。

use crate::provider::{ChatMessage, model_cards};

/// 未知窗口时的字符预算,与旧 `MAX_CONTEXT_CHARS` 一致。
pub(crate) const DEFAULT_CHARS: usize = 48_000;
/// 窗口使用比例:留出输出与系统提示的余量。
const WINDOW_RATIO: f64 = 0.6;
/// 运营上限:即便模型标称 1M 窗口,也不放任单次上下文无限增长(约 18 万中文字)。
const MAX_TOKENS: usize = 120_000;
/// 预算下限:窗口再小也要保证一轮完整对话放得下。
const MIN_TOKENS: usize = 2_000;

/// 预算单位:窗口未知时沿用字符口径,已知时按估算 token。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limit {
    Chars(usize),
    Tokens(usize),
}

/// 本次请求的上下文预算。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextBudget {
    limit: Limit,
}

impl ContextBudget {
    /// 查不到模型窗口时的旧口径(48k 字符)。
    pub(crate) fn unknown() -> Self {
        Self {
            limit: Limit::Chars(DEFAULT_CHARS),
        }
    }

    /// 按 provider + 模型 id 计算预算(查不到卡片时回落 `unknown`)。
    pub(crate) fn for_model(provider_id: &str, model: &str) -> Self {
        match context_window_for(provider_id, model) {
            Some(window) => {
                let tokens =
                    ((window as f64 * WINDOW_RATIO) as usize).clamp(MIN_TOKENS, MAX_TOKENS);
                Self {
                    limit: Limit::Tokens(tokens),
                }
            }
            None => Self::unknown(),
        }
    }

    /// 预算上限(与 `weight` 同单位)。
    pub(crate) fn limit(&self) -> usize {
        match self.limit {
            Limit::Chars(n) | Limit::Tokens(n) => n,
        }
    }

    /// 单条消息计入预算的权重。
    pub(crate) fn weight(&self, m: &ChatMessage) -> usize {
        match self.limit {
            Limit::Chars(_) => m.content.chars().count(),
            Limit::Tokens(_) => message_tokens(m),
        }
    }

    /// 整组消息是否超出预算。
    pub(crate) fn over(&self, messages: &[ChatMessage]) -> bool {
        messages.iter().map(|m| self.weight(m)).sum::<usize>() > self.limit()
    }

    /// 预算口径与上限(供事件日志记录,便于事后核对本轮压缩决策的依据)。
    pub(crate) fn describe(&self) -> (&'static str, usize) {
        match self.limit {
            Limit::Chars(n) => ("chars", n),
            Limit::Tokens(n) => ("tokens", n),
        }
    }

    /// 估算是否按 token 口径(仅测试断言用)。
    #[cfg(test)]
    pub(crate) fn is_token_based(&self) -> bool {
        matches!(self.limit, Limit::Tokens(_))
    }
}

/// 匹配模型卡的上下文窗口:先精确(provider + 官方 id),再按规范化 id 跨 provider 匹配
/// (中转站常把官方模型挂到自定义 provider 下)。
fn context_window_for(provider_id: &str, model: &str) -> Option<u64> {
    let cards = model_cards::list_model_cards();
    let target = normalize(model);
    cards
        .iter()
        .find(|c| c.provider == provider_id && normalize(&c.official_model_id) == target)
        .or_else(|| {
            cards
                .iter()
                .find(|c| normalize(&c.official_model_id) == target)
        })
        .and_then(|c| c.context_window)
        .filter(|w| *w > 0)
}

/// 模型 id 规范化:`models/` 前缀、`provider/` 前缀、大小写差异都不影响匹配。
fn normalize(model: &str) -> String {
    let s = model.trim().trim_start_matches("models/");
    let tail = s.rsplit('/').next().unwrap_or(s);
    tail.to_ascii_lowercase()
}

/// token 估算:CJK 按 1.5 字符/token,其余按 4 字符/token(向上取整)。
pub(crate) fn estimate_tokens(text: &str) -> usize {
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

/// 单条消息的 token 估算:正文 + 工具调用参数(参数里的长文本同样占上下文)。
fn message_tokens(m: &ChatMessage) -> usize {
    let mut n = estimate_tokens(&m.content);
    if let Some(calls) = &m.tool_calls {
        for c in calls {
            n += estimate_tokens(&c.function.arguments) + 8;
        }
    }
    // 角色与协议开销
    n + 4
}

/// 整组消息的 token 估算(供事件日志上报「这一轮大概占多少上下文」)。
pub(crate) fn messages_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(message_tokens).sum()
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2E80..=0x9FFF   // 中日韩统一表意文字 + 部首
            | 0xAC00..=0xD7AF // 韩文音节
            | 0xF900..=0xFAFF // 兼容表意文字
            | 0xFF00..=0xFFEF // 全角符号
    )
}

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

    #[test]
    fn estimates_tokens_by_character_mix() {
        assert_eq!(estimate_tokens(""), 0);
        // 100 个英文 ≈ 25 token
        assert_eq!(estimate_tokens(&"a".repeat(100)), 25);
        // 150 个中文 ≈ 100 token
        assert_eq!(estimate_tokens(&"中".repeat(150)), 100);
        // 中英混合:1.5 与 4 分别折算后相加
        assert!(estimate_tokens("中文abc") > 0);
    }

    #[test]
    fn unknown_model_falls_back_to_legacy_char_budget() {
        let b = ContextBudget::for_model("my-custom-provider", "no-such-model-xyz");
        assert!(!b.is_token_based());
        assert_eq!(b.limit(), DEFAULT_CHARS);

        let mut msgs = vec![msg("system", "s")];
        for _ in 0..10 {
            msgs.push(msg("user", &"中".repeat(4_000)));
        }
        assert!(!b.over(&msgs), "40k 字符仍在 48k 字符预算内");
        msgs.push(msg("user", &"中".repeat(10_000)));
        assert!(b.over(&msgs));
    }

    #[test]
    fn known_model_uses_window_share_and_is_capped() {
        // 32k 窗口 → 60% ≈ 19.6k token 预算
        let small = ContextBudget::for_model("qwen", "qwen3.6-flash");
        assert!(small.is_token_based());
        assert_eq!(small.limit(), 19_660);
        // 1M 窗口受运营上限约束,避免单次请求无限膨胀
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
        // 字符口径只计正文,工具参数不计;token 口径会计入
        assert!(b.weight(&plain) <= b.weight(&with_calls));
        let tok = ContextBudget {
            limit: Limit::Tokens(10_000),
        };
        assert!(tok.weight(&with_calls) > tok.weight(&plain));
    }
}
