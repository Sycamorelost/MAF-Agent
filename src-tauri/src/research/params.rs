//! 研究可配置参数与提示词/资料装配。

use serde::{Deserialize, Serialize};

use super::{stage, template, EngineParams};

/// 深度研究可配置参数(迭代 D4)。
///
/// 与前端 ResearchPage 配置栏一一对应;同时是研究预设(presets.json)的载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResearchParams {
    /// 研究深度:ReAct 循环最大轮次(1-25)
    pub max_rounds: u32,
    /// 每轮查询数(1-10)
    pub queries_per_round: u32,
    /// 信息源模式:web(全网)/ academic(学术优先)/ whitelist(域名白名单)
    pub source_mode: String,
    /// source_mode = whitelist 时生效的域名列表
    pub whitelist_domains: Vec<String>,
    /// 单源阅读长度:digest(摘要 240 字)/ full(扩展 1200 字)
    pub read_length: String,
    /// 反思策略:every(每轮反思)/ every3(每 3 轮)/ final(仅开头规划)
    pub reflect: String,
    /// 最大检索次数(0 = 不限)
    pub max_searches: u32,
    /// Token 预算上限,按字符估算(0 = 不限)
    pub token_budget: u32,
    /// 输出语言:zh / en
    pub output_lang: String,
    /// 专项研究:全局决策提示覆盖(None = templates.json 自定义 > 内置)
    #[serde(default)]
    pub decide_prompt: Option<String>,
    /// 专项研究:终稿报告模板覆盖(None = templates.json 自定义 > 内置)
    #[serde(default)]
    pub report_prompt: Option<String>,
    /// 自由结构:true = 不使用报告模板,AI 根据主题与资料自行决定章节结构
    #[serde(default)]
    pub free_structure: bool,
    /// 研究模式:legacy(快速研究,单层 ReAct)/ staged(专项研究,阶段流水线)
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 专项模式阶段编排(mode=staged 时生效;L4:legacy 模式忽略,后端仅做归一化校验)
    #[serde(default)]
    pub stages: Vec<stage::StageConfig>,
    /// 检查点模式:每阶段结束暂停等用户确认(L4:staged 专属;legacy 模式忽略,后端仅透传)
    #[serde(default)]
    pub checkpoint: bool,
    /// 全局总时长上限(分钟,0 = 不限;超限视同预算耗尽,撰写阶段与终稿仍执行)
    #[serde(default)]
    pub max_duration_minutes: u32,
    /// 全局资料条数上限(0 = 不限)
    #[serde(default)]
    pub max_sources: u32,
    /// 同轮查询并发数 1-5(默认 1 顺序执行;429 风险场景保持 1)
    /// (L4:staged 专属;legacy 模式忽略,后端仅 clamp)
    #[serde(default = "default_concurrent_queries")]
    pub concurrent_queries: u32,
    /// 单查询失败策略:skip 跳过继续 / abort_stage 中止本阶段 / abort_research 中止研究
    /// (L4:staged 专属;legacy 模式忽略,非法值回退 skip)
    #[serde(default = "default_fail_policy")]
    pub fail_policy: String,
    /// URL 去重范围:global 全程去重 / stage 仅阶段内去重(允许跨阶段复检同一来源)
    /// (L4:staged 专属;legacy 模式忽略,非法值回退 global)
    #[serde(default = "default_dedup_scope")]
    pub dedup_scope: String,
    /// 科研增强(v1.9.0):注入内置研究方法论技能(默认开,轻量片段)
    #[serde(default = "default_true")]
    pub skill_enhance: bool,
    /// 深度审计:终稿追加引用与统计审计附录(默认关;skill_enhance 开启时生效)
    #[serde(default)]
    pub skill_audit: bool,
    /// AI 概念示意草稿:终稿追加一张生图草稿并标注(默认关;skill_enhance 开启时生效)
    #[serde(default)]
    pub skill_image_draft: bool,
    /// 草稿生图 Provider id(skill_image_draft=true 时必填)
    #[serde(default)]
    pub skill_image_provider: Option<String>,
    /// 草稿生图模型名(skill_image_draft=true 时必填)
    #[serde(default)]
    pub skill_image_model: Option<String>,
    /// 用户导入的参考资料(仅文件名,落盘于 data/research/references/;严禁路径分隔符)
    #[serde(default)]
    pub reference_files: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_concurrent_queries() -> u32 {
    1
}

fn default_fail_policy() -> String {
    "skip".into()
}

fn default_dedup_scope() -> String {
    "global".into()
}

fn default_mode() -> String {
    "legacy".into()
}

impl Default for ResearchParams {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            queries_per_round: 4,
            source_mode: "web".into(),
            whitelist_domains: Vec::new(),
            read_length: "digest".into(),
            reflect: "every".into(),
            max_searches: 0,
            token_budget: 120_000,
            output_lang: "zh".into(),
            decide_prompt: None,
            report_prompt: None,
            free_structure: false,
            mode: default_mode(),
            stages: Vec::new(),
            checkpoint: false,
            max_duration_minutes: 0,
            max_sources: 0,
            concurrent_queries: 1,
            fail_policy: default_fail_policy(),
            dedup_scope: default_dedup_scope(),
            skill_enhance: true,
            skill_audit: false,
            skill_image_draft: false,
            skill_image_provider: None,
            skill_image_model: None,
            reference_files: Vec::new(),
        }
    }
}

impl ResearchParams {
    /// 单源摘要注入的最大字符数(「单源阅读长度」参数)。
    pub fn snippet_chars(&self) -> usize {
        if self.read_length == "full" {
            1200
        } else {
            240
        }
    }

    /// 把用户的原始查询按信息源模式改写为实际下发的查询词。
    pub(crate) fn rewrite_query(&self, q: &str) -> String {
        match self.source_mode.as_str() {
            "academic" => format!("{q} 学术 论文 研究"),
            "whitelist" if !self.whitelist_domains.is_empty() => {
                let sites = self
                    .whitelist_domains
                    .iter()
                    .map(|d| format!("site:{d}"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("{q} ({sites})")
            }
            _ => q.to_string(),
        }
    }

    /// 生效决策模板:逐次覆盖(decide_prompt)> templates.json 自定义 > 内置。
    pub fn decide_template(&self) -> String {
        self.decide_prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| template::effective("decide", &self.output_lang))
    }

    /// 生效终稿模板:逐次覆盖(report_prompt)> templates.json 自定义 > 内置。
    pub fn report_template(&self) -> String {
        self.report_prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| template::effective("report", &self.output_lang))
    }

    /// 终稿 system 提示:free_structure = true 时忽略模板,给最小自由结构指令;
    /// 返回 (system 提示, 是否含 {references} 引用锚点)。自由结构恒无锚点(finalize 统一追加资料来源)。
    pub fn report_system(&self, topic: &str, stages_digest: &str) -> (String, bool) {
        let lang = if self.output_lang == "en" { "English" } else { "中文" };
        if self.free_structure {
            let mut sys = format!(
                "你是一名严谨的研究撰写助手。请输出一篇完整、深入的 Markdown 研究报告。\n\n\
                 研究主题:{topic}\n输出语言:{lang}\n\n\
                 要求:\n\
                 1. 报告的章节结构与组织方式由你根据主题特点和资料情况自行决定,不套用固定模板;\n\
                 2. 引用资料中的论断以 [n] 标注,编号与资料列表一一对应;不得虚构资料之外的引用编号;\n\
                 3. 只输出 Markdown 正文,不要代码围栏,不要前言。"
            );
            if !stages_digest.trim().is_empty() {
                sys.push_str("\n\n【阶段摘要】\n");
                sys.push_str(stages_digest);
            }
            (sys, false)
        } else {
            let tpl = self.report_template();
            let anchor = tpl.contains("{references}");
            let rendered = template::render(
                &tpl,
                &[
                    ("topic", topic.to_string()),
                    ("output_lang", lang.to_string()),
                    ("stages_digest", stages_digest.to_string()),
                    ("prev_summaries", stages_digest.to_string()),
                ],
            );
            (rendered, anchor)
        }
    }
}

/// 把参考资料注入块追加到 system prompt(无参考资料时原样返回)。
pub(crate) fn with_references(mut system: String, p: &EngineParams) -> String {
    if let Some(b) = p.references.as_deref() {
        system.push_str("\n\n");
        system.push_str(b);
    }
    system
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_constraints_serde_defaults() {
        // 旧 presets.json 无全局约束字段时 serde default 兜底(步骤 3)
        let p: ResearchParams = serde_json::from_str(r#"{"max_rounds":8}"#).unwrap();
        assert_eq!(p.concurrent_queries, 1);
        assert_eq!(p.fail_policy, "skip");
        assert_eq!(p.dedup_scope, "global");
        assert_eq!(p.max_duration_minutes, 0);
        assert_eq!(p.max_sources, 0);
    }
}

