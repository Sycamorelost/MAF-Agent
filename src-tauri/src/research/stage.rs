//! 专项研究:阶段定义与阶段内约束(升级步骤 2)。
//!
//! - StageConfig:阶段流水线中的一环(检索 research / 撰写 synthesis);
//! - StageConstraints:阶段内精细约束,合并优先级「阶段约束 > 阶段参数覆盖 > 全局 > 内置」;
//! - stages.json:阶段编排快照 + 运行期状态,落盘 `data/research/{id}/stages.json`(文件即事实源)。

use serde::{Deserialize, Serialize};

use super::ResearchParams;

/// 阶段配置(前端编排 → ResearchParams.stages 载荷)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StageConfig {
    /// 阶段内唯一 id(前端生成,排序/引用用)
    pub id: String,
    /// 阶段名,如「背景与现状摸底」
    pub name: String,
    /// 阶段目标,注入决策提示
    pub goal: String,
    /// research=检索阶段 / synthesis=纯撰写阶段(不检索,只消费资料)
    pub kind: String,
    /// 停用则跳过(不删除,便于方案复用)
    pub enabled: bool,
    // ---- 参数覆盖(None = 跟随全局默认)----
    pub max_rounds: Option<u32>,
    pub queries_per_round: Option<u32>,
    pub source_mode: Option<String>,
    pub whitelist_domains: Option<Vec<String>>,
    pub read_length: Option<String>,
    pub reflect: Option<String>,
    // ---- 提示词覆盖 ----
    /// 本阶段决策提示(覆盖全局)
    pub decide_prompt: Option<String>,
    /// 阶段摘要提示(覆盖内置)
    pub summary_prompt: Option<String>,
    // ---- 阶段内约束 ----
    pub constraints: StageConstraints,
}

impl Default for StageConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            goal: String::new(),
            kind: "research".into(),
            enabled: true,
            max_rounds: None,
            queries_per_round: None,
            source_mode: None,
            whitelist_domains: None,
            read_length: None,
            reflect: None,
            decide_prompt: None,
            summary_prompt: None,
            constraints: StageConstraints::default(),
        }
    }
}

/// 阶段内约束(全部字段为空/默认值时表示不施加额外约束)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StageConstraints {
    /// 本阶段 Token 上限(0/None = 不限;全局预算仍是硬上限)
    pub stage_token_budget: Option<u32>,
    /// 本阶段检索次数上限
    pub stage_max_searches: Option<u32>,
    /// 本阶段新增资料上限
    pub stage_max_sources: Option<u32>,
    /// 资料数下限:未达标自动追加 1 轮(最多补 2 轮)并在摘要中标注缺口
    pub min_sources: Option<u32>,
    /// 必覆盖角度:决策提示中列为硬性要求,结束时校验,缺失发 AngleGap 提醒(不阻断)
    pub must_cover_angles: Vec<String>,
    /// 本阶段域名白名单(source_mode=whitelist 且未覆盖 whitelist_domains 时生效)
    pub include_domains: Vec<String>,
    /// 域名黑名单:检索结果过滤
    pub exclude_domains: Vec<String>,
    /// 查询词语言倾向:auto / zh / en(注入决策提示)
    pub query_language: String,
    /// 允许模型判定「资料足够」提前 finish(false = 必须跑满轮次)
    pub early_stop_on_enough: bool,
    /// 决策上下文注入前阶段摘要(默认开)
    pub inject_prev_summaries: bool,
    /// 决策上下文可见前阶段资料明细(默认关:只见摘要,控 token)
    pub inherit_sources: bool,
}

impl Default for StageConstraints {
    fn default() -> Self {
        Self {
            stage_token_budget: None,
            stage_max_searches: None,
            stage_max_sources: None,
            min_sources: None,
            must_cover_angles: Vec::new(),
            include_domains: Vec::new(),
            exclude_domains: Vec::new(),
            query_language: "auto".into(),
            early_stop_on_enough: true,
            inject_prev_summaries: true,
            inherit_sources: false,
        }
    }
}

/// 三级合并后的阶段生效参数(阶段约束/覆盖 > 全局 > 内置默认值已在 ResearchParams 体现)。
#[derive(Debug, Clone)]
pub struct EffStage {
    pub max_rounds: u32,
    pub queries_per_round: u32,
    pub source_mode: String,
    pub whitelist_domains: Vec<String>,
    pub read_length: String,
    pub reflect: String,
    /// 本阶段检索次数上限(0 = 不限)
    pub max_searches: u32,
    /// 本阶段 Token 上限(0 = 不限)
    pub token_budget: u32,
    /// 本阶段新增资料上限(0 = 不限)
    pub max_sources: u32,
    /// 资料数下限(0 = 不校验)
    pub min_sources: u32,
    pub must_cover_angles: Vec<String>,
    pub exclude_domains: Vec<String>,
    pub query_language: String,
    pub early_stop_on_enough: bool,
    pub inject_prev_summaries: bool,
    pub inherit_sources: bool,
}

impl StageConfig {
    pub fn is_synthesis(&self) -> bool {
        self.kind == "synthesis"
    }

    /// 合并全局参数与本阶段覆盖/约束,产出本阶段生效参数。
    pub fn effective(&self, g: &ResearchParams) -> EffStage {
        let c = &self.constraints;
        // 白名单优先级:阶段参数覆盖 > 阶段约束 include_domains > 全局
        let whitelist = self
            .whitelist_domains
            .clone()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                if c.include_domains.is_empty() {
                    None
                } else {
                    Some(c.include_domains.clone())
                }
            })
            .unwrap_or_else(|| g.whitelist_domains.clone());
        EffStage {
            max_rounds: self.max_rounds.unwrap_or(g.max_rounds).clamp(1, 25),
            queries_per_round: self
                .queries_per_round
                .unwrap_or(g.queries_per_round)
                .clamp(1, 10),
            source_mode: self
                .source_mode
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| g.source_mode.clone()),
            whitelist_domains: whitelist,
            read_length: self
                .read_length
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| g.read_length.clone()),
            reflect: self
                .reflect
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| g.reflect.clone()),
            max_searches: c.stage_max_searches.unwrap_or(0),
            token_budget: c.stage_token_budget.unwrap_or(0),
            max_sources: c.stage_max_sources.unwrap_or(0),
            min_sources: c.min_sources.unwrap_or(0),
            must_cover_angles: c.must_cover_angles.clone(),
            exclude_domains: c.exclude_domains.clone(),
            query_language: c.query_language.clone(),
            early_stop_on_enough: c.early_stop_on_enough,
            inject_prev_summaries: c.inject_prev_summaries,
            inherit_sources: c.inherit_sources,
        }
    }

    /// 按生效参数改写查询词(信息源模式 + 查询语言倾向)。
    pub fn rewrite_query_eff(eff: &EffStage, q: &str) -> String {
        let base = match eff.source_mode.as_str() {
            "academic" => format!("{q} 学术 论文 研究"),
            "whitelist" if !eff.whitelist_domains.is_empty() => {
                let sites = eff
                    .whitelist_domains
                    .iter()
                    .map(|d| format!("site:{d}"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("{q} ({sites})")
            }
            _ => q.to_string(),
        };
        match eff.query_language.as_str() {
            "zh" => format!("{base} 中文资料"),
            "en" => format!("{base} English"),
            _ => base,
        }
    }
}

/// 从 URL 中提取 host(小写);不依赖 url crate,非法输入返回空串。
fn host_of(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or_default();
    // 去掉可能的 userinfo 与端口
    let host = rest.rsplit('@').next().unwrap_or(rest);
    let host = host.split(':').next().unwrap_or_default();
    host.to_lowercase()
}

/// 检查 url 是否命中域名黑名单(按 host 后缀匹配)。
pub fn domain_blocked(url: &str, exclude: &[String]) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let host = host_of(url);
    if host.is_empty() {
        return false;
    }
    exclude.iter().any(|d| {
        let d = d.trim().to_lowercase();
        !d.is_empty() && (host == d || host.ends_with(&format!(".{d}")))
    })
}

// ---------------- stages.json(阶段编排快照 + 运行期状态) ----------------

/// 单个阶段的运行期状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageState {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    /// pending / running / done / skipped
    pub status: String,
    pub summary: String,
    pub sources_count: u32,
    pub rounds: u32,
    /// M2:因失败策略提前中止的阶段(摘要基于部分资料);serde default 兼容旧 stages.json
    #[serde(default)]
    pub partial: bool,
}

/// stages.json 文件结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagesFile {
    pub mode: String,
    pub stages: Vec<StageState>,
    pub updated: String,
    /// H1:完整阶段配置快照(检查点「调整下阶段」的写回目标;
    /// 引擎每阶段起点热读覆盖,serde default 兼容旧 stages.json)
    #[serde(default)]
    pub configs: Vec<StageConfig>,
}

impl StagesFile {
    /// 从阶段编排初始化(全部 pending;停用的直接 skipped)。
    pub fn from_configs(mode: &str, stages: &[StageConfig]) -> Self {
        Self {
            mode: mode.to_string(),
            stages: stages
                .iter()
                .map(|s| StageState {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    kind: s.kind.clone(),
                    enabled: s.enabled,
                    status: if s.enabled { "pending" } else { "skipped" }.into(),
                    summary: String::new(),
                    sources_count: 0,
                    rounds: 0,
                    partial: false,
                })
                .collect(),
            updated: chrono::Local::now().to_rfc3339(),
            configs: stages.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global() -> ResearchParams {
        ResearchParams {
            max_rounds: 8,
            queries_per_round: 4,
            source_mode: "web".into(),
            whitelist_domains: vec!["global.example".into()],
            read_length: "digest".into(),
            reflect: "every".into(),
            ..ResearchParams::default()
        }
    }

    #[test]
    fn effective_inherits_global_when_no_override() {
        let s = StageConfig::default();
        let eff = s.effective(&global());
        assert_eq!(eff.max_rounds, 8);
        assert_eq!(eff.queries_per_round, 4);
        assert_eq!(eff.whitelist_domains, vec!["global.example".to_string()]);
        assert_eq!(eff.max_searches, 0);
        assert!(eff.early_stop_on_enough);
        assert!(eff.inject_prev_summaries);
        assert!(!eff.inherit_sources);
    }

    #[test]
    fn effective_override_beats_global_and_constraints_win() {
        let mut s = StageConfig::default();
        s.max_rounds = Some(6);
        s.constraints.stage_max_searches = Some(12);
        s.constraints.include_domains = vec!["arxiv.org".into()];
        s.source_mode = Some("whitelist".into());
        let eff = s.effective(&global());
        assert_eq!(eff.max_rounds, 6);
        assert_eq!(eff.max_searches, 12);
        // include_domains 优先于全局白名单
        assert_eq!(eff.whitelist_domains, vec!["arxiv.org".to_string()]);
        // 阶段参数覆盖又优先于 include_domains
        s.whitelist_domains = Some(vec!["github.com".into()]);
        let eff2 = s.effective(&global());
        assert_eq!(eff2.whitelist_domains, vec!["github.com".to_string()]);
    }

    #[test]
    fn rewrite_query_applies_language_hint() {
        let s = StageConfig::default();
        let mut eff = s.effective(&global());
        assert_eq!(StageConfig::rewrite_query_eff(&eff, "LLM 推理"), "LLM 推理");
        eff.query_language = "zh".into();
        assert!(StageConfig::rewrite_query_eff(&eff, "LLM 推理").contains("中文资料"));
        eff.query_language = "en".into();
        assert!(StageConfig::rewrite_query_eff(&eff, "LLM 推理").contains("English"));
        eff.source_mode = "whitelist".into();
        let q = StageConfig::rewrite_query_eff(&eff, "KV cache");
        assert!(q.contains("site:global.example"));
    }

    #[test]
    fn domain_blocked_matches_host_and_subdomain() {
        let ex = vec!["zhihu.com".to_string(), "csdn.net".to_string()];
        assert!(domain_blocked("https://zhihu.com/a/b", &ex));
        assert!(domain_blocked("https://www.zhihu.com/a", &ex));
        assert!(domain_blocked("https://blog.csdn.net/x", &ex));
        assert!(!domain_blocked("https://arxiv.org/abs/1", &ex));
        assert!(!domain_blocked("not a url", &ex));
        assert!(!domain_blocked("https://zhihu.com", &[]));
    }

    #[test]
    fn stage_config_serde_defaults() {
        // 旧 presets.json 无 stages 字段时 serde default 兜底
        let s: StageConfig = serde_json::from_str(r#"{"name":"摸底"}"#).unwrap();
        assert_eq!(s.kind, "research");
        assert!(s.enabled);
        assert_eq!(s.constraints.query_language, "auto");
    }
}
