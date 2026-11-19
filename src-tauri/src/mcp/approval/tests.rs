//! 审批策略与风险判定回归测试。

use super::rules::{assess_risk, in_workspace_at, workspace_write_auto_allowed};
use super::*;
use serde_json::json;

#[test]
fn risk_distinguishes_readonly_from_write() {
    // 内置只读工具:名字由本产品定义,可判低风险(工作区档自动放行)
    assert_eq!(
        assess_risk("builtin", "fs_read", &json!({})),
        ApprovalRisk::Low
    );
    assert_eq!(
        assess_risk("builtin", "fs_grep", &json!({})),
        ApprovalRisk::Low
    );
    assert_eq!(
        assess_risk("builtin", "fs_write", &json!({})),
        ApprovalRisk::High
    );
    assert_eq!(
        assess_risk("builtin", "fs_delete", &json!({})),
        ApprovalRisk::High
    );
    // M2:present 只登记产物,不写文件 → 低风险,不触发审批
    assert_eq!(
        assess_risk("builtin", "present", &json!({})),
        ApprovalRisk::Low
    );

    // P1:外部 MCP 工具名由对方 server 提供,不可作为放行依据 —— 一律至少 Medium;
    // 即便名字里含 search/read 也不得判 Low(否则 `search_and_wipe` 可绕过审批)。
    for name in [
        "read_text_file",
        "search_nodes",
        "search_and_wipe",
        "get_everything",
        "sequentialthinking",
        "create_entities",
    ] {
        assert_eq!(
            assess_risk("mcp", name, &json!({})),
            ApprovalRisk::Medium,
            "外部工具 `{name}` 不得按名字判为低风险"
        );
    }
    // 外部工具的破坏性动词仍直接判高风险
    assert_eq!(
        assess_risk("mcp", "delete_entities", &json!({})),
        ApprovalRisk::High
    );
    // 敏感路径:即便只读工具也按高风险处理
    assert_eq!(
        assess_risk("mcp", "read_text_file", &json!({ "path": "/a/.env" })),
        ApprovalRisk::High
    );
}

#[test]
fn policy_modes_gate_approval() {
    let ask = ApprovalPolicy {
        mode: ApprovalMode::Ask,
        allow: vec![],
    };
    assert!(ask.requires_approval("s", "read_text_file", ApprovalRisk::Low, &json!({})));

    let ws = ApprovalPolicy {
        mode: ApprovalMode::Workspace,
        allow: vec![],
    };
    // 只读免打扰
    assert!(!ws.requires_approval("s", "read_text_file", ApprovalRisk::Low, &json!({})));
    // shell / 删除 / 外部写 一律审批(workspace_write_auto_allowed 对非白名单工具直接 false,
    // 不触碰文件系统,可在测试中安全调用)
    assert!(ws.requires_approval(
        "builtin",
        "shell_exec",
        ApprovalRisk::High,
        &json!({"command":"ls"})
    ));
    assert!(ws.requires_approval(
        "builtin",
        "fs_delete",
        ApprovalRisk::High,
        &json!({"path":"a.txt"})
    ));
    assert!(ws.requires_approval(
        "mcp",
        "write_file",
        ApprovalRisk::High,
        &json!({"path":"a.txt"})
    ));

    let full = ApprovalPolicy {
        mode: ApprovalMode::Full,
        allow: vec![],
    };
    assert!(!full.requires_approval("s", "shell_exec", ApprovalRisk::High, &json!({})));

    // 旧自定义白名单 → workspace 档附加放行
    let ws_allow = ApprovalPolicy {
        mode: ApprovalMode::Workspace,
        allow: vec!["项目文件::read_text_file".into(), "search_nodes".into()],
    };
    assert!(!ws_allow.requires_approval(
        "项目文件",
        "read_text_file",
        ApprovalRisk::Low,
        &json!({})
    ));
    assert!(!ws_allow.requires_approval(
        "长期记忆",
        "search_nodes",
        ApprovalRisk::Low,
        &json!({})
    ));
    assert!(ws_allow.requires_approval(
        "长期记忆",
        "create_entities",
        ApprovalRisk::Medium,
        &json!({})
    ));
}

/// P7:工作区档下,凭据类文件名即使在工作区内、即使命中自定义白名单也必须审批。
#[test]
fn workspace_forces_approval_for_credential_paths() {
    let ws = ApprovalPolicy {
        mode: ApprovalMode::Workspace,
        // 即使把 fs_write 加进白名单,也不得绕过凭据文件门控
        allow: vec!["fs_write".into()],
    };
    for path in [
        ".env",
        "config/.env.local",
        "C:\\ws\\credentials.json",
        "deploy/id_rsa",
        "certs/server.pem",
        "private/app.key",
    ] {
        assert!(
            ws.requires_approval(
                "builtin",
                "fs_write",
                ApprovalRisk::High,
                &json!({ "path": path, "content": "x" })
            ),
            "凭据类路径必须审批: {path}"
        );
    }
    // 不含敏感词的路径不会被这条规则误伤(是否自动放行仍由工作区边界决定)
    assert!(!sensitive_path(&json!({ "path": "notes/keyboard.md" })));
}

#[test]
fn workspace_write_boundary_check() {
    // 纯函数判定:相对路径落在工作区内,绝对越界路径不在
    let root = std::env::temp_dir().join(format!("maf-ws-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    assert!(in_workspace_at(&root, "out/a.txt"));
    assert!(in_workspace_at(&root, "sub/../a.txt"));
    let outside = std::env::temp_dir().join(format!("maf-out-{}", uuid::Uuid::new_v4()));
    assert!(!in_workspace_at(&root, outside.to_str().unwrap()));
    // 越过工作区的相对路径(.. 跳不出去时才算在内;跳出根目录即判不在)
    assert!(!in_workspace_at(&root, "../../../../etc/passwd"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn workspace_allows_chunk_tools_only_with_live_write_id() {
    // 未 begin / 会话已结束时 chunk 与 finish 不可免审批
    assert!(!workspace_write_auto_allowed(
        "builtin",
        "fs_write_chunk",
        &json!({ "write_id": "no-such-id" })
    ));
    assert!(!workspace_write_auto_allowed(
        "builtin",
        "fs_write_finish",
        &json!({})
    ));
    // 外部 MCP 的同名工具不享受内置白名单
    assert!(!workspace_write_auto_allowed(
        "mcp",
        "fs_write_chunk",
        &json!({ "write_id": "x" })
    ));
    // 写/编辑类必须显式给出 path
    assert!(!workspace_write_auto_allowed(
        "builtin",
        "fs_write",
        &json!({ "content": "x" })
    ));
    assert!(!workspace_write_auto_allowed(
        "builtin",
        "fs_edit",
        &json!({ "old": "a", "new": "b" })
    ));
}

/// 审批档是全局永久设置:保存后重新读取(模拟重启)仍保持同一档位,且不受会话影响。
#[test]
fn approval_mode_is_permanent_and_global() {
    let dir = std::env::temp_dir().join(format!("maf-mode-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let full = ApprovalPolicy {
        mode: ApprovalMode::Full,
        allow: vec![],
    };
    save_policy(&dir, &full).unwrap();
    let reloaded = load_policy(&dir);
    assert_eq!(reloaded.mode, ApprovalMode::Full);
    // 任意会话上下文生效档位一致,不存在临时授权回落
    assert_eq!(effective_mode(&reloaded, &dir, None), ApprovalMode::Full);
    assert_eq!(
        effective_mode(&reloaded, &dir, Some("s1")),
        ApprovalMode::Full
    );
    assert_eq!(
        effective_mode(&reloaded, &dir, Some("s2")),
        ApprovalMode::Full
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn policy_json_roundtrip_and_legacy_alias() {
    let p: ApprovalPolicy =
        serde_json::from_str(r#"{"mode":"full","allow":["a::b"]}"#).unwrap();
    assert_eq!(p.mode, ApprovalMode::Full);
    assert_eq!(p.allow, vec!["a::b".to_string()]);
    // 旧值别名迁移:manual→ask,auto/custom→workspace;full 保持 full(绝不降级)
    let m: ApprovalPolicy = serde_json::from_str(r#"{"mode":"manual"}"#).unwrap();
    assert_eq!(m.mode, ApprovalMode::Ask);
    let a: ApprovalPolicy = serde_json::from_str(r#"{"mode":"auto"}"#).unwrap();
    assert_eq!(a.mode, ApprovalMode::Workspace);
    let c: ApprovalPolicy = serde_json::from_str(r#"{"mode":"custom","allow":["x"]}"#).unwrap();
    assert_eq!(c.mode, ApprovalMode::Workspace);
    assert_eq!(c.allow, vec!["x".to_string()]);
    // 缺失/损坏 → 逐次审批档(默认),不缺省直放也不全弹窗
    let d: ApprovalPolicy = serde_json::from_str("{}").unwrap();
    assert_eq!(d.mode, ApprovalMode::Ask);
}
