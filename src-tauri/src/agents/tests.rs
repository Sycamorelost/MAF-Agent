use super::*;

#[test]
fn builtin_has_22_unique_agents() {
    let list = builtin_templates();
    assert_eq!(list.len(), 22);
    let mut ids: Vec<&str> = list.iter().map(|t| t.agent_id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 22, "agent_id 必须唯一");
    for t in &list {
        assert!(
            CATEGORIES.contains(&t.category.as_str()),
            "非法分类:{}",
            t.category
        );
        assert!(["full", "summary", "none"].contains(&t.memory_strategy.as_str()));
        assert!(t.builtin);
        assert!(
            !t.system_prompt.trim().is_empty(),
            "{} 缺 system_prompt",
            t.agent_id
        );
        assert!(!t.input_hints.is_empty(), "{} 缺 input_hints", t.agent_id);
    }
    // R4:内置模板收缩为 22 个后,不再要求每个分类至少 4 个;
    // 只校验分类分布与设计一致,防止后续增删时悄悄漂移。
    let expect = [
        ("chat", 3),
        ("doc", 4),
        ("dev", 2),
        ("create", 2),
        ("ops", 1),
        ("meta", 2),
        ("study", 2),
        ("work", 2),
        ("academic", 4),
    ];
    for (c, want) in expect {
        let n = list.iter().filter(|t| t.category == c).count();
        assert_eq!(n, want, "分类 {c} 应有 {want} 个内置模板");
    }
    let bare = list
        .iter()
        .find(|t| t.agent_id == "bare-assistant")
        .expect("缺少 bare-assistant");
    assert_eq!(bare.category, "meta");
    assert_eq!(bare.memory_strategy, "full");
}

/// R4:每个内置 id 都必须有对应提示词,且提示词表不再有多余分支。
#[test]
fn builtin_prompts_match_templates() {
    for t in builtin_templates() {
        assert!(
            !crate::agents_prompts::prompt_for(&t.agent_id).is_empty(),
            "{} 缺少提示词",
            t.agent_id
        );
    }
    assert!(
        crate::agents_prompts::prompt_for("diff-reader").is_empty(),
        "未注册的 diff-reader 不应再有提示词"
    );
}

/// R3 B4:去 AI 味 4 专用智能体,tool_policy/doc_pipeline 按 val.md §5.4 表。
#[test]
fn builtin_aiflavor_agents_policy() {
    let list = builtin_templates();
    let expect = [
        ("ai-flavor-detector", vec!["fs_read"], "inline"),
        ("ai-flavor-rewriter", vec!["fs_read", "fs_write"], "inline"),
        ("academic-voice", vec!["fs_read"], "resident"),
        ("style-anchor", vec![], "off"),
    ];
    for (id, tools, pipeline) in expect {
        let t = list
            .iter()
            .find(|t| t.agent_id == id)
            .unwrap_or_else(|| panic!("缺少 {id}"));
        let want: Vec<String> = tools.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            t.tool_policy.as_deref(),
            Some(want.as_slice()),
            "{id} tool_policy 不符"
        );
        assert_eq!(t.doc_pipeline, pipeline, "{id} doc_pipeline 不符");
        assert_eq!(t.memory_strategy, "none");
        assert!(!t.memory_write, "{id} 不应写记忆");
        // v2 字段消费收口(R3-D):四个入口智能体必须有开场白与追问建议
        assert!(!t.opening_message.trim().is_empty(), "{id} 缺少开场白");
        assert!(!t.suggestions.is_empty(), "{id} 缺少追问建议");
    }
}

/// 记忆读写矩阵:strategy=none 的模板不允许写记忆;full/summary 必须可写。
#[test]
fn builtin_memory_matrix_consistent() {
    for t in builtin_templates() {
        if t.memory_strategy == "none" {
            assert!(!t.memory_write, "{} strategy=none 不应写记忆", t.agent_id);
        } else {
            assert!(
                t.memory_write,
                "{} strategy={} 应可写记忆",
                t.agent_id, t.memory_strategy
            );
        }
    }
}

#[test]
fn custom_json_roundtrip() {
    let v = AgentTemplate {
        agent_id: "product-assistant--deep".into(),
        name: "产品助手-深度版".into(),
        icon: "sparkles".into(),
        description: "变体".into(),
        category: "chat".into(),
        recommended_model: "deepseek-v3".into(),
        memory_strategy: "summary".into(),
        system_prompt: "base prompt\n\n追加后缀".into(),
        input_hints: vec!["hi".into()],
        memory_write: true,
        memory_sections: vec!["会话摘要".into()],
        base_agent: Some("product-assistant".into()),
        builtin: false,
        ..v2_defaults()
    };
    let text = serde_json::to_string(&[v]).unwrap();
    let back: Vec<AgentTemplate> = serde_json::from_str(&text).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].base_agent.as_deref(), Some("product-assistant"));
    assert!(!back[0].builtin);
}

/// 旧版/缺字段 JSON 能解析为默认(本地优先不报错)。
#[test]
fn custom_json_missing_fields_default() {
    let back: Vec<AgentTemplate> = serde_json::from_str(
        r#"[{"agent_id":"x","name":"x","icon":"bot","description":"","category":"chat",
            "recommended_model":"m","memory_strategy":"full","system_prompt":"p"}]"#,
    )
    .unwrap();
    assert!(back[0].input_hints.is_empty());
    assert!(!back[0].memory_write);
    assert!(back[0].base_agent.is_none());
}

/// 变体解析:基座字段继承 + system_prompt 后缀拼接(前端创建时已解析,此处验证拼接约定)。
#[test]
fn variant_prompt_suffix_concat() {
    let base = find("product-assistant").unwrap();
    let variant_prompt = format!("{}\n\n所有回复使用简体中文。", base.system_prompt);
    assert!(variant_prompt.starts_with(&base.system_prompt));
    assert!(variant_prompt.ends_with("简体中文。"));
}

/// B4 / F2:模板变量渲染({date}/{model}/{language}),未知变量保留。
#[test]
fn render_prompt_substitutes_known_vars() {
    let out = render_prompt(
        "今天是{date},模型{model},请用{language}回复。{unknown}",
        "gpt-x",
    );
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        out,
        format!("今天是{today},模型gpt-x,请用中文回复。{{unknown}}")
    );
}

#[test]
fn render_prompt_without_vars_unchanged() {
    assert_eq!(render_prompt("普通提示词", "m"), "普通提示词");
}
