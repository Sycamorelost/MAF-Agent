//! delegate:把独立子任务委派给专项智能体。子调用不带工具,天然防递归。

use serde_json::Value;

pub(super) async fn exec(
    args: &Value,
    sub_llm: Option<&crate::provider::SubLlm>,
) -> (String, bool) {
    let query = args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if task.is_empty() {
        return ("缺少 task 参数".into(), false);
    }
    let Some(llm) = sub_llm else {
        return ("委派通道不可用(当前 provider 未配置 API Key)".into(), false);
    };
    // 先按 agent_id 精确找,再按名称找(用户自定义与内置同名时以 find 结果为准)
    let agent = crate::agents::find(query).or_else(|| {
        crate::agents::list_all()
            .into_iter()
            .find(|a| a.name == query)
    });
    let Some(agent) = agent else {
        return (
            format!("未找到智能体 `{query}`。可用 delegate 的 agent 参数填智能体名称或 id。"),
            false,
        );
    };
    let sys = crate::agents::render_prompt(&agent.system_prompt, &llm.model);
    let msgs = vec![
        crate::provider::ChatMessage {
            role: "system".into(),
            content: sys,
            ..Default::default()
        },
        crate::provider::ChatMessage {
            role: "user".into(),
            content: task.chars().take(20_000).collect(),
            ..Default::default()
        },
    ];
    match llm.collect(msgs, 4096).await {
        Ok(text) => {
            let text = text.trim();
            if text.is_empty() {
                return (format!("子智能体「{}」返回了空内容", agent.name), false);
            }
            let truncated: String = text.chars().take(32_000).collect();
            (
                format!("【子智能体「{}」的执行结果】\n{}", agent.name, truncated),
                true,
            )
        }
        Err(e) => (format!("子智能体「{}」执行失败:{e}", agent.name), false),
    }
}
