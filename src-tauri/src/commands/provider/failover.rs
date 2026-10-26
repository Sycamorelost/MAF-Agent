//! Key 轮换执行与错误分类 / 冷却策略。

use crate::provider::{
    ProviderError, keychain,
    rotation::{KeyCandidate, RotationConfig, RotationState},
};

/// 非流式辅助请求(模型列表 / 图片生成)的 Key 轮换执行器:
/// 依次尝试各可用候选,遇限流 / 5xx / 网络错误则冷却该槽位并换下一把 Key。
/// 与对话共用同一 group 的冷却表,避免刚被限流的 Key 立刻又被辅助请求撞上。
pub(crate) async fn with_key_rotation<T, F, Fut>(
    group: &str,
    cands: Vec<KeyCandidate>,
    state: &RotationState,
    cfg: &RotationConfig,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut(&KeyCandidate, String) -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    if cands.is_empty() {
        return Err("未配置 API Key".into());
    }
    let mut pool = cands;
    let mut last_error: Option<String> = None;
    for _ in 0..pool.len().max(1) {
        if pool.is_empty() {
            break;
        }
        let idx = match state.pick(group, &pool) {
            Some(i) => i,
            None => break,
        };
        let cand = pool[idx].clone();
        let key = match keychain::load_slot(&cand.provider_id, cand.slot) {
            Ok(Some(k)) => k,
            Ok(None) => {
                pool.retain(|c| c.cooldown_key() != cand.cooldown_key());
                continue;
            }
            Err(e) => return Err(e),
        };
        match attempt(&cand, key).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_error = Some(e.to_string());
                if !is_retryable(&e) {
                    break;
                }
                let cool = cooldown_for(&e, cfg.cooldown_secs);
                state.mark_cooldown_secs(group, &cand.provider_id, cand.slot, cool);
                pool.retain(|c| c.cooldown_key() != cand.cooldown_key());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "所有 API Key 均不可用(可能全部被限流)".into()))
}

/// 请求唯一 id(毫秒时间戳 + 进程内自增序号)。
pub(crate) fn next_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn error_kind(e: &ProviderError) -> &'static str {
    match e {
        ProviderError::RateLimited { .. } => "rate_limited",
        ProviderError::Auth(_) => "auth",
        ProviderError::Server(_) => "server",
        ProviderError::Network(_) => "network",
        ProviderError::BadRequest(_) => "bad_request",
        ProviderError::NotSupported(_) => "not_supported",
        ProviderError::Parse(_) => "parse",
        ProviderError::Other(_) => "other",
    }
}

/// 是否值得换槽重试(限流 / 服务端错误 / 网络错误)。
pub(crate) fn is_retryable(e: &ProviderError) -> bool {
    matches!(
        e,
        ProviderError::RateLimited { .. } | ProviderError::Server(_) | ProviderError::Network(_)
    )
}

/// 429 的建议冷却秒数:优先用响应头 Retry-After,缺失时回退到配置值。
pub(crate) fn cooldown_for(e: &ProviderError, cfg_secs: u64) -> u64 {
    match e {
        ProviderError::RateLimited { retry_after, .. } => retry_after.unwrap_or(cfg_secs).max(1),
        _ => 10,
    }
}

/// 该错误是否由「中转站不认 stream_options」引起(据此去掉字段重试一次)。
pub(crate) fn is_stream_options_rejected(e: &ProviderError) -> bool {
    match e {
        ProviderError::BadRequest(m) => {
            let m = m.to_ascii_lowercase();
            m.contains("stream_option")
        }
        _ => false,
    }
}

/// 该错误是否由「端点不认思考扩展参数」引起(据此剥离 reasoning 重试一次,v1.9.0 工作块 I)。
/// 典型报错关键词:reasoning_effort / thinking / enable_thinking(各家扩展字段名)。
pub(crate) fn is_reasoning_rejected(e: &ProviderError) -> bool {
    match e {
        ProviderError::BadRequest(m) => {
            let m = m.to_ascii_lowercase();
            m.contains("reasoning_effort")
                || m.contains("reasoning")
                || m.contains("thinking")
                || m.contains("enable_thinking")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中转站不认 stream_options 的 400 文案可被识别,用于回退重试。
    #[test]
    fn detect_stream_options_rejection() {
        assert!(is_stream_options_rejected(&ProviderError::BadRequest(
            "Unrecognized request argument supplied: stream_options".into()
        )));
        assert!(!is_stream_options_rejected(&ProviderError::BadRequest(
            "invalid model".into()
        )));
        assert!(!is_stream_options_rejected(&ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None
        }));
    }

    /// 限流 / 5xx / 网络错误可换槽重试;鉴权错误不可。
    #[test]
    fn retryable_classification() {
        assert!(is_retryable(&ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None
        }));
        assert!(is_retryable(&ProviderError::Server("503".into())));
        assert!(is_retryable(&ProviderError::Network("timeout".into())));
        assert!(!is_retryable(&ProviderError::Auth("401".into())));
        assert!(!is_retryable(&ProviderError::BadRequest("400".into())));
    }

    /// Phase 2:429 冷却优先采用 Retry-After,缺失时回退配置值;其他错误固定 10s。
    #[test]
    fn cooldown_prefers_retry_after_header() {
        let with = ProviderError::RateLimited {
            message: "429".into(),
            retry_after: Some(42),
        };
        assert_eq!(cooldown_for(&with, 60), 42);
        let without = ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None,
        };
        assert_eq!(cooldown_for(&without, 60), 60);
        assert_eq!(cooldown_for(&ProviderError::Server("503".into()), 60), 10);
    }
}
