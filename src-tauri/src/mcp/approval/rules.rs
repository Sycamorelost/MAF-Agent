//! 风险判定与工作区边界等纯函数(不触碰 IO,可单测)。

use super::ApprovalRisk;

/// 工作区档的写操作免审批判定(D11):仅限内置文本/文档/表格写工具,
/// 且 path 解析后落在工作区内;删除 / 备份恢复 / zip 解包 / shell 一律仍需审批。
/// 分批写入的 chunk/finish 不带 path,改为校验 write_id 有效
/// (目标路径已在 begin 时做过同样的工作区校验,且分片只落临时文件,无法改道)。
#[allow(dead_code)] // 单测使用;运行时统一走会话感知版
pub(super) fn workspace_write_auto_allowed(
    server: &str,
    tool: &str,
    args: &serde_json::Value,
) -> bool {
    workspace_write_auto_allowed_in(None, server, tool, args)
}

/// 会话感知版:工作区边界按会话绑定的项目目录解析(未绑定项目时回退全局)。
pub(super) fn workspace_write_auto_allowed_in(
    session_id: Option<&str>,
    server: &str,
    tool: &str,
    args: &serde_json::Value,
) -> bool {
    if server != "builtin" {
        return false;
    }
    if matches!(tool, "fs_write_chunk" | "fs_write_finish") {
        return args
            .get("write_id")
            .and_then(|v| v.as_str())
            .map(crate::fs::text::write_chunk_known)
            .unwrap_or(false);
    }
    if !matches!(
        tool,
        "fs_write" | "fs_edit" | "fs_write_begin" | "fs_export_docx" | "fs_export_xlsx"
    ) {
        return false;
    }
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return false;
    };
    let Ok(root) = crate::fs::paths::resolve_workspace_root(session_id) else {
        return false;
    };
    in_workspace_at(&root, path)
}

/// 路径是否落在指定工作区内(相对路径按工作区解析,绝对路径直接判定;解析失败保守按不在)。
pub(super) fn in_workspace_at(root: &std::path::Path, path: &str) -> bool {
    let Ok(abs) = crate::fs::paths::resolve(root, path, false) else {
        return false;
    };
    crate::fs::paths::rel_of(root, &abs).is_ok()
}

/// P7:参数中的 path 是否命中凭据类敏感文件名(工作区档也必须审批)。
pub(super) fn sensitive_path(args: &serde_json::Value) -> bool {
    args.as_object()
        .and_then(|obj| obj.get("path"))
        .and_then(|v| v.as_str())
        .map(|path| {
            let p = path.replace('\\', "/").to_ascii_lowercase();
            [
                ".env",
                "credentials",
                "secrets",
                "id_rsa",
                ".pem",
                ".pfx",
                ".key",
            ]
            .iter()
            .any(|k| p.contains(k))
        })
        .unwrap_or(false)
}

/// 风险判定。
///
/// 内置工具名由本产品定义,可按动词推断;外部 MCP 工具名由对方 server 自行返回,
/// 不可信的字符串不能作为放行依据 —— 外部工具一律至少 Medium(逐次弹窗),
/// 「看起来只读」只用于弹窗文案,免打扰交给白名单 / 本会话「始终允许」。
pub(crate) fn assess_risk(server: &str, tool: &str, args: &serde_json::Value) -> ApprovalRisk {
    // 1. 敏感路径参数直接高风险
    if sensitive_path(args) {
        return ApprovalRisk::High;
    }
    let t = tool.to_ascii_lowercase();
    // M2:present 只登记产物元数据(不写用户文件、不改内容),按低风险处理,
    // 否则每轮交付都会弹审批,打断「写完即交付」的主流程。
    if server == "builtin" && t == "present" {
        return ApprovalRisk::Low;
    }
    // 2. 破坏性/执行类动词 → 高风险
    const DESTRUCTIVE: [&str; 7] = [
        "delete",
        "write",
        "edit",
        "patch",
        "move",
        "shell_exec",
        "execute",
    ];
    if DESTRUCTIVE.iter().any(|k| t.contains(k)) {
        return ApprovalRisk::High;
    }
    // 3. 外部 server 的工具名不可信:不因名字含 read/search/get 就判低风险
    if server != "builtin" {
        return ApprovalRisk::Medium;
    }
    // 4. 内置只读工具 → 低风险(工作区档自动放行)
    if looks_readonly(&t) {
        return ApprovalRisk::Low;
    }
    // 5. 其余(create/add/copy… 有副作用但非破坏性)→ 中风险
    ApprovalRisk::Medium
}

/// 名字启发式:工具名是否「看起来只读」。仅用于提示文案,不作为放行依据。
fn looks_readonly(tool_lower: &str) -> bool {
    const READONLY: [&str; 9] = [
        "read", "list", "search", "get", "open", "query", "retrieve", "tree", "grep",
    ];
    READONLY
        .iter()
        .any(|k| tool_lower.starts_with(k) || tool_lower.contains(k))
}

pub(super) fn reason_for_risk(server: &str, risk: &ApprovalRisk, tool: &str) -> String {
    // 外部工具即使名字像只读,也必须说明「名字不可信」,避免用户被文案误导
    if server != "builtin" && looks_readonly(&tool.to_ascii_lowercase()) {
        return format!(
            "外部 MCP 工具 `{tool}` 的名字看起来像只读,但工具名由该 server 自行提供,\
             请核对参数后再放行"
        );
    }
    match risk {
        ApprovalRisk::High => format!("工具 `{tool}` 涉及高风险操作,需要您的确认"),
        ApprovalRisk::Medium => format!("工具 `{tool}` 涉及文件/命令操作,建议检查参数后放行"),
        ApprovalRisk::Low => format!("工具 `{tool}` 为只读查询类操作"),
    }
}
