//! usage 查询与聚合单测(内存库)。

use super::*;
use super::entries::{entries_on, key_health_on};
use super::hourly::hourly_on;
use crate::db::schema;

fn mem() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    schema::migrate(&conn).unwrap();
    conn
}

fn rec(status: &str, slot: u32, tokens: i64, first_ms: Option<i64>) -> super::super::UsageRecord {
    super::super::UsageRecord {
        request_id: format!("r{slot}-{status}"),
        provider_id: "openai".into(),
        provider_name: Some("OpenAI".into()),
        model_id: "gpt-4o".into(),
        slot_id: slot,
        slot_label: Some(format!("key-{slot}")),
        key_masked: Some("sk-a****1234".into()),
        status: status.into(),
        total_tokens: Some(tokens as u32),
        first_token_ms: first_ms,
        ..Default::default()
    }
}

#[test]
fn insert_and_aggregate_by_model() {
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 100, Some(200))).unwrap();
    super::super::insert_on(&conn, &rec("success", 1, 300, Some(400))).unwrap();
    super::super::insert_on(&conn, &rec("rate_limited", 2, 0, None)).unwrap();

    let sum = summary_on(&conn, "model", 7).unwrap();
    assert_eq!(sum.totals.requests, 3);
    assert_eq!(sum.totals.tokens, 400);
    assert_eq!(sum.totals.failed, 1);
    assert_eq!(sum.totals.rate_limited, 1);
    assert_eq!(sum.totals.avg_first_token_ms, Some(300));
    assert_eq!(sum.totals.successful_requests, 2);
    assert_eq!(sum.totals.uncosted_requests, 2);
    assert_eq!(sum.totals.cost_coverage, 0.0);
    assert_eq!(sum.groups.len(), 1);
    assert_eq!(sum.groups[0].key, "gpt-4o");
    assert_eq!(sum.groups[0].tokens, 400);
}

#[test]
fn aggregate_by_provider_and_key() {
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 100, Some(200))).unwrap();
    super::super::insert_on(&conn, &rec("success", 1, 300, Some(400))).unwrap();

    let by_p = summary_on(&conn, "provider", 7).unwrap();
    assert_eq!(by_p.groups.len(), 1);
    assert_eq!(by_p.groups[0].key, "openai");
    assert_eq!(by_p.groups[0].name, "OpenAI");

    let by_k = summary_on(&conn, "key", 7).unwrap();
    assert_eq!(by_k.groups.len(), 2);
    // 槽位 1 token 更多 → 排在首位
    assert_eq!(by_k.groups[0].slot_id, Some(1));
    assert_eq!(by_k.groups[1].name, "key-0");
}

#[test]
fn key_health_counts_per_slot() {
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 10, Some(1))).unwrap();
    super::super::insert_on(&conn, &rec("rate_limited", 0, 0, None)).unwrap();
    super::super::insert_on(&conn, &rec("failed", 1, 0, None)).unwrap();

    let h = key_health_on(&conn, "openai", 7).unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].slot_id, Some(0));
    assert_eq!(h[0].requests, 2);
    assert_eq!(h[0].rate_limited, 1);
    assert_eq!(h[1].failed, 1);
}

#[test]
fn entries_are_newest_first_and_filtered() {
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 1, None)).unwrap();
    super::super::insert_on(&conn, &rec("success", 1, 2, None)).unwrap();
    let e = entries_on(&conn, 10, 7, None, None).unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].slot_id, 1);

    // 维度下钻下沉 SQL:只返回该分组
    let only0 = entries_on(&conn, 10, 7, Some("key"), Some("0")).unwrap();
    assert_eq!(only0.len(), 1);
    assert_eq!(only0[0].slot_id, 0);
    // 非数字槽位键 → 空结果
    assert!(entries_on(&conn, 10, 7, Some("key"), Some("abc")).unwrap().is_empty());
}

/// Phase 2:按助手归因 + 成本聚合(纳元口径)。
#[test]
fn aggregate_by_agent_and_cost() {
    let conn = mem();
    let mut a = rec("success", 0, 100, Some(200));
    a.agent_id = Some("chat".into());
    a.agent_name = Some("通用助手".into());
    a.cost_nano = Some(1_200_000);
    let mut b = rec("success", 1, 300, Some(400));
    b.agent_id = Some("chat".into());
    b.agent_name = Some("通用助手".into());
    b.cost_nano = Some(800_000);
    // 未绑定助手的请求归入 'none'
    super::super::insert_on(&conn, &a).unwrap();
    super::super::insert_on(&conn, &b).unwrap();
    super::super::insert_on(&conn, &rec("rate_limited", 2, 0, None)).unwrap();

    let by_agent = summary_on(&conn, "agent", 7).unwrap();
    assert_eq!(by_agent.groups.len(), 2);
    let chat = by_agent.groups.iter().find(|g| g.key == "chat").unwrap();
    assert_eq!(chat.name, "通用助手");
    assert_eq!(chat.requests, 2);
    assert_eq!(chat.cost_nano, 2_000_000);
    let none = by_agent.groups.iter().find(|g| g.key == "none").unwrap();
    assert_eq!(none.name, "未绑定助手");

    let by_model = summary_on(&conn, "model", 7).unwrap();
    assert_eq!(by_model.totals.cost_nano, 2_000_000);
    assert_eq!(by_model.totals.costed_requests, 2);
    assert_eq!(by_model.totals.uncosted_requests, 0);
    assert_eq!(by_model.totals.cost_coverage, 1.0);
}

/// 旧记录只有 cost_micro 时按 ×1000 兜底,避免历史成本消失。
#[test]
fn legacy_cost_micro_falls_back_to_nano() {
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 10, None)).unwrap();
    conn.execute("UPDATE usage_record SET cost_micro = 5", []).unwrap();
    let sum = summary_on(&conn, "model", 7).unwrap();
    assert_eq!(sum.totals.cost_nano, 5_000);
    assert_eq!(sum.totals.cost_coverage, 1.0);
}

/// >50 个分类时追加「其他」桶,保证 Σ分组 = 总览。
#[test]
fn other_bucket_balances_totals() {
    let conn = mem();
    for i in 0..60 {
        let mut r = rec("success", 0, 1, None);
        r.model_id = format!("m{i}");
        r.cost_nano = Some(1000);
        super::super::insert_on(&conn, &r).unwrap();
    }
    let sum = summary_on(&conn, "model", 7).unwrap();
    assert_eq!(sum.groups.len(), 51);
    assert_eq!(sum.groups.last().unwrap().key, "__other__");
    let g_requests: i64 = sum.groups.iter().map(|g| g.requests).sum();
    let g_tokens: i64 = sum.groups.iter().map(|g| g.tokens).sum();
    let g_cost: i64 = sum.groups.iter().map(|g| g.cost_nano).sum();
    assert_eq!(g_requests, sum.totals.requests);
    assert_eq!(g_tokens, sum.totals.tokens);
    assert_eq!(g_cost, sum.totals.cost_nano);
}

/// Phase 2:热力图分桶(dow × hour)。
#[test]
fn hourly_buckets_created_records() {
    use chrono::{Datelike, TimeZone, Timelike};
    let conn = mem();
    super::super::insert_on(&conn, &rec("success", 0, 100, Some(10))).unwrap();
    let buckets = hourly_on(&conn, 7).unwrap();
    let total_req: i64 = buckets.iter().map(|b| b.requests).sum();
    let total_tok: i64 = buckets.iter().map(|b| b.tokens).sum();
    assert_eq!(total_req, 1);
    assert_eq!(total_tok, 100);
    let now = chrono::Local.timestamp_opt(now_secs(), 0).single().unwrap();
    let expect = buckets.iter().find(|b| {
        b.dow == now.weekday().num_days_from_sunday() as i64 && b.hour == now.hour() as i64
    });
    assert!(expect.is_some(), "应落在当前小时对应的格子");
}
