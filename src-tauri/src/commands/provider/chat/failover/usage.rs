//! 用量落库:成功流的成本归因,以及失败/限流尝试的记录。

use super::super::super::keys::ChatSpec;
use super::super::super::pricing::cost_nano_of;
use super::Attribution;
use crate::db::usage::{self, UsageRecord};
use crate::provider::rotation::KeyCandidate;
use std::time::Instant;

/// 流正常结束后落一条用量记录(含成本归因)。
#[allow(clippy::too_many_arguments)]
pub(super) fn record_success(
    request_id: &str,
    started: &Instant,
    spec: &ChatSpec,
    cand: &KeyCandidate,
    provider_name: Option<String>,
    label: Option<String>,
    masked: String,
    attr: &Attribution,
    toks: (Option<u32>, Option<u32>, Option<u32>),
    reasoning_toks: Option<u32>,
    cached_toks: Option<u32>,
    first_token_ms: Option<i64>,
    stream_err: Option<&String>,
) {
    // 成本(纳元):有定价但 token 不足无法拆分时记 unpriced,不写 0 元假账
    let cost_nano = attr
        .pricing
        .as_ref()
        .and_then(|p| cost_nano_of(p, toks.0, toks.1, cached_toks));
    let (pricing_snapshot, cost_source) = match (&attr.pricing, cost_nano) {
        (Some(p), Some(_)) => (Some(p.snapshot.clone()), Some(p.source.clone())),
        (Some(p), None) => (Some(p.snapshot.clone()), Some("unpriced".into())),
        (None, _) => (None, Some("none".into())),
    };
    usage::record(&UsageRecord {
        request_id: request_id.to_string(),
        session_id: attr.session_id.clone(),
        provider_id: cand.provider_id.clone(),
        provider_name,
        model_id: spec.model.clone(),
        slot_id: cand.slot,
        slot_label: label,
        key_masked: Some(masked),
        status: if stream_err.is_none() {
            "success"
        } else {
            "failed"
        }
        .into(),
        error_kind: stream_err.as_ref().map(|_| "stream".to_string()),
        input_tokens: toks.0,
        output_tokens: toks.1,
        total_tokens: toks.2,
        reasoning_tokens: reasoning_toks,
        cached_tokens: cached_toks,
        first_token_ms,
        total_ms: Some(started.elapsed().as_millis() as i64),
        agent_id: attr.agent_id.clone(),
        agent_name: attr.agent_name.clone(),
        cost_nano,
        pricing_snapshot,
        cost_source,
    });
}

/// 一次失败(含限流)尝试的记录。
pub(super) fn record_failure(
    request_id: &str,
    started: &Instant,
    spec: &ChatSpec,
    cand: &KeyCandidate,
    provider_name: Option<String>,
    label: Option<String>,
    masked: String,
    attr: &Attribution,
    kind: &str,
) {
    usage::record(&UsageRecord {
        request_id: request_id.to_string(),
        session_id: attr.session_id.clone(),
        provider_id: cand.provider_id.clone(),
        provider_name,
        model_id: spec.model.clone(),
        slot_id: cand.slot,
        slot_label: label,
        key_masked: Some(masked),
        status: if kind == "rate_limited" {
            "rate_limited"
        } else {
            "failed"
        }
        .into(),
        error_kind: Some(kind.to_string()),
        total_ms: Some(started.elapsed().as_millis() as i64),
        agent_id: attr.agent_id.clone(),
        agent_name: attr.agent_name.clone(),
        cost_source: Some("none".into()),
        ..Default::default()
    });
}
