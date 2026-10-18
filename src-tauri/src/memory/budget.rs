//! memory.md 注入预算护栏(迭代 D3.5)。
//!
//! 预算规则(idea/03-memory-flow.md §2.6):注入 LLM 的记忆正文不超过 2000 tokens。
//! token 估算为轻量启发式:字符数 / 2 上取整(中英混排下的保守估计),
//! 前端 src/lib/memoryInject.ts 保持同一规则,两端结果一致。

/// 注入预算上限(tokens)。
pub const MEMORY_BUDGET_TOKENS: usize = 2000;

/// 估算文本 token 数:字符数 / 2 上取整。
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_half_of_chars_ceil() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 2);
        assert_eq!(estimate_tokens("abc"), 2); // 3 / 2 上取整
        assert_eq!(estimate_tokens("记忆宫殿"), 2); // 4 个中文字符
    }
}
