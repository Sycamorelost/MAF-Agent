//! V16–V17 迁移 SQL。

/// V16(学习规划板块 P1):自学单元 / 测验 / 测验作答 / 复习队列四表。
/// study_units 每个知识点一份自学材料(UNIQUE 幂等);quizzes 每次生成一行(保留作答历史);
/// reviews 每个知识点一行 SM-2-lite 排期(due 为 YYYY-MM-DD,便于字符串比较)。
pub(super) const MIGRATION_STUDY_P1: &str = r#"
CREATE TABLE IF NOT EXISTS study_units (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  node_id INTEGER NOT NULL,
  node_name TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL DEFAULT '',
  est_minutes INTEGER NOT NULL DEFAULT 30,
  content_md TEXT NOT NULL DEFAULT '',
  created TEXT NOT NULL,
  UNIQUE(plan_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_units_plan ON study_units(plan_id);

CREATE TABLE IF NOT EXISTS quizzes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  node_id INTEGER NOT NULL,
  node_name TEXT NOT NULL DEFAULT '',
  payload_json TEXT NOT NULL DEFAULT '[]',
  created TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_quizzes_plan ON quizzes(plan_id);

CREATE TABLE IF NOT EXISTS quiz_attempts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  quiz_id INTEGER NOT NULL REFERENCES quizzes(id) ON DELETE CASCADE,
  question_idx INTEGER NOT NULL,
  user_answer TEXT NOT NULL DEFAULT '',
  correct INTEGER NOT NULL DEFAULT 0,
  answered_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_attempts_quiz ON quiz_attempts(quiz_id);

CREATE TABLE IF NOT EXISTS reviews (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  node_id INTEGER NOT NULL,
  due TEXT NOT NULL,               -- YYYY-MM-DD,到期日
  interval_days INTEGER NOT NULL DEFAULT 1,
  last_result INTEGER NOT NULL DEFAULT 0, -- 1 记得 / 0 忘了
  UNIQUE(plan_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_reviews_plan ON reviews(plan_id, due);
"#;

/// V17(计量口径):纯时间范围查询走索引。
pub(super) const MIGRATION_V17_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_usage_time ON usage_record(created_at);";

/// V18(维度复合索引):按 agent 维度分组 + 时间窗口查询。
/// usage_record 已有 idx_usage_pm(provider_id,model_id,created_at)/
/// idx_usage_slot(slot_id,created_at)/idx_usage_time(created_at),
/// 唯独 agent_id + created_at 组合缺失,summary_by(agent) 会全表扫描。
pub(super) const MIGRATION_V18_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_usage_agent_time ON usage_record(agent_id, created_at);";

/// V19(harness 任务清单):每会话一份 todo,模型整表替换式写入,UI 实时展示。
pub(super) const MIGRATION_V19_TODOS: &str = r#"
CREATE TABLE IF NOT EXISTS todos (
  session_id TEXT NOT NULL,
  item_id    TEXT NOT NULL,
  content    TEXT NOT NULL,
  status     TEXT NOT NULL DEFAULT 'pending',
  priority   TEXT NOT NULL DEFAULT 'medium',
  ord        INTEGER NOT NULL DEFAULT 0,
  updated    INTEGER NOT NULL,
  PRIMARY KEY (session_id, item_id)
);
"#;

/// V20(PERF-3):两个高频列表查询的支撑索引。
///   - idx_sessions_updated:list_sessions 的 `ORDER BY updated DESC` 不再全表排序;
///   - idx_messages_attachments:list_all_attachments 的「有附件」过滤 + id 倒序
///     由部分索引直接覆盖,避免对 messages 全表扫描。
pub(super) const MIGRATION_V20_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated DESC);
CREATE INDEX IF NOT EXISTS idx_messages_attachments ON messages(id DESC)
  WHERE attachments IS NOT NULL AND attachments <> '[]';
"#;
