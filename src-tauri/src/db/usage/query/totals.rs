//! 总览聚合(UsageTotals)。

use rusqlite::{params, Connection};

use super::{UsageTotals, COST};

pub(super) fn totals_on(conn: &Connection, since: i64) -> rusqlite::Result<UsageTotals> {
    let sql = format!(
        "SELECT COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END),
                CAST(AVG(CASE WHEN status = 'success' THEN first_token_ms END) AS INTEGER),
                COALESCE(SUM({COST}), 0),
                SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'success' AND {COST} IS NOT NULL THEN 1 ELSE 0 END)
         FROM usage_record WHERE created_at >= ?1"
    );
    conn.query_row(&sql, params![since], |r| {
        let successful: i64 = r.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let costed: i64 = r.get::<_, Option<i64>>(7)?.unwrap_or(0);
        Ok(UsageTotals {
            requests: r.get(0)?,
            tokens: r.get(1)?,
            failed: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            rate_limited: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            avg_first_token_ms: r.get(4)?,
            cost_nano: r.get(5)?,
            successful_requests: successful,
            costed_requests: costed,
            uncosted_requests: (successful - costed).max(0),
            cost_coverage: if successful > 0 {
                costed as f64 / successful as f64
            } else {
                0.0
            },
        })
    })
}
