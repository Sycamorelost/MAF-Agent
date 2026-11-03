//! Agent 模板结构与分类、v2 字段默认值、提示词变量渲染。

use serde::{Deserialize, Serialize};

/// Agent 模板(内置与自定义共用同一结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    pub agent_id: String,
    pub name: String,
    /// 图标 key,前端映射到线性 SVG(禁 emoji)
    pub icon: String,
    pub description: String,
    /// chat / doc / dev / create / ops / meta
    pub category: String,
    pub recommended_model: String,
    /// 记忆注入策略:full / summary / none(绑定会话时写入 sessions.memory_injection_strategy)
    pub memory_strategy: String,
    pub system_prompt: String,
    #[serde(default)]
    pub input_hints: Vec<String>,
    /// 是否参与记忆自动提炼(写记忆)
    #[serde(default)]
    pub memory_write: bool,
    /// 提炼结果写入 memory.md 的哪些节(展示用)
    #[serde(default)]
    pub memory_sections: Vec<String>,
    /// 变体的基座模板 id(非变体为 None)
    #[serde(default)]
    pub base_agent: Option<String>,
    /// 是否内置(内置不可删除/覆盖);JSON 中缺省 false
    #[serde(default)]
    pub builtin: bool,
    // —— v2 字段(R3,val.md §4.3):全部有默认,旧 agents.json 兼容 ——
    /// 开场白:空会话首条展示(None = 不展示)
    #[serde(default)]
    pub opening_message: String,
    /// 输入框追问建议(chips)
    #[serde(default)]
    pub suggestions: Vec<String>,
    /// 工具策略:None=继承全局;Some(空)=禁用工具;Some(列表)=仅允许名单内工具
    #[serde(default)]
    pub tool_policy: Option<Vec<String>>,
    /// 思考档位偏好:空 = 跟随用户设置;off / low / medium / high
    #[serde(default)]
    pub thinking_level: String,
    /// 采样温度偏好(None = 跟随模型默认)
    #[serde(default)]
    pub temperature: Option<f32>,
    /// 附件文档管线:inline(直接注入,现行为)/ resident(驻留+检索,待 Phase C)/ off
    #[serde(default)]
    pub doc_pipeline: String,
    /// 触发词(Skill 层,命中时提示切换智能体;Phase A 仅存储)
    #[serde(default)]
    pub triggers: Vec<String>,
    /// 模板默认运行时预设(P5 / D19):None = 跟随全局默认;economy / standard / deep / code
    #[serde(default)]
    pub preset: Option<String>,
    /// 存储结构:None / "coupled" = 单文件 agents.json 数组项;"decoupled" = 目录多文件(prompt.md/config.json/tools.json)。
    /// 缺省为 coupled,旧数据自动兼容。
    #[serde(default)]
    pub storage: Option<String>,
    /// 解耦存储下关联的外部 prompt 文件绝对路径(None = 使用内部 prompt.md)。
    /// 由用户在编辑器中选择,保存时写入该文件,加载时从该文件读取。
    #[serde(default)]
    pub prompt_file: Option<String>,
    /// 解耦存储下关联的外部 tools 文件绝对路径(None = 使用内部 tools.json 或无)。
    #[serde(default)]
    pub tools_file: Option<String>,
    /// 运行时标记(不落盘语义):该自定义模板是否覆盖了同 id 的内置模板。
    /// 由 list_all 填充,用于 UI 显示「恢复内置」。
    #[serde(skip)]
    pub overrides_builtin: bool,
}

/// 合法分类(R3 A3 扩展:生活/学习/工作/学术四大方向)。
pub const CATEGORIES: [&str; 10] = [
    "chat", "doc", "dev", "create", "ops", "meta", "life", "study", "work", "academic",
];

/// v2 新增字段的空默认:保持 t() 原签名,内置模板仅显式覆盖需要的字段。
pub(crate) fn v2_defaults() -> AgentTemplate {
    AgentTemplate {
        agent_id: String::new(),
        name: String::new(),
        icon: String::new(),
        description: String::new(),
        category: String::new(),
        recommended_model: String::new(),
        memory_strategy: "full".into(),
        system_prompt: String::new(),
        input_hints: Vec::new(),
        memory_write: false,
        memory_sections: Vec::new(),
        base_agent: None,
        builtin: false,
        opening_message: String::new(),
        suggestions: Vec::new(),
        tool_policy: None,
        thinking_level: String::new(),
        temperature: None,
        doc_pipeline: String::new(),
        triggers: Vec::new(),
        preset: None,
        storage: None,
        prompt_file: None,
        tools_file: None,
        overrides_builtin: false,
    }
}

/// 渲染 system_prompt 模板变量(迭代 B4 / F2):
/// `{date}` → 今天日期(YYYY-MM-DD);`{model}` → 会话当前模型 id;`{language}` → 中文。
/// 未知变量原样保留;绑定会话或「应用为提示词」时渲染一次后落盘。
pub fn render_prompt(template: &str, model: &str) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    template
        .replace("{date}", &date)
        .replace("{model}", model)
        .replace("{language}", "中文")
}
