//! GUI 自动化内置工具:定义下发 + 执行入口。
//!
//! 全部工具先校验 GUI 策略(开关 + 进程白名单),再操作系统;
//! 只读的窗口/元素查询不弹审批,动作类(点击/写值/输入/聚焦)必须审批或持能力令牌。

use serde_json::{Value, json};

use crate::provider::{ToolDef, ToolFunctionDef};

use super::registry::REGISTRY;
use super::{policy, uia};

/// 全部 GUI 工具名(与 dispatcher 的白名单保持一致)。
pub const GUI_TOOL_NAMES: [&str; 6] = [
    "ui_list_windows",
    "ui_find_elements",
    "ui_invoke",
    "ui_set_value",
    "ui_type_text",
    "ui_focus_window",
];

pub fn is_gui_tool(name: &str) -> bool {
    GUI_TOOL_NAMES.contains(&name)
}

/// 工具定义(仅在 GUI 自动化开启时随对话下发)。
pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "ui_list_windows",
            "列出当前可见的桌面窗口(hwnd / 进程名 / 标题)。GUI 自动化前先用它确认目标窗口与进程是否在白名单内。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "ui_find_elements",
            "在指定窗口内按名称 / AutomationId / 控件类型搜索 UI 元素,返回 element_id 供后续点击或写值。\
             平台为 Windows UI Automation,仅能访问白名单进程。",
            json!({
                "type": "object",
                "properties": {
                    "hwnd": { "type": "number", "description": "窗口句柄(来自 ui_list_windows)" },
                    "name": { "type": "string", "description": "元素名称(包含匹配,可选)" },
                    "automation_id": { "type": "string", "description": "AutomationId(包含匹配,可选)" },
                    "control_type": { "type": "string", "description": "控件类型,如 button / edit / menuitem(可选)" },
                    "limit": { "type": "number", "description": "最多返回数量,默认 20" }
                },
                "required": ["hwnd"]
            }),
        ),
        tool(
            "ui_invoke",
            "触发一个 UI 元素:优先使用 Invoke/Toggle/SelectionItem 模式,不支持时按元素中心模拟单击。",
            json!({
                "type": "object",
                "properties": { "element_id": { "type": "string", "description": "ui_find_elements 返回的短句柄" } },
                "required": ["element_id"]
            }),
        ),
        tool(
            "ui_set_value",
            "向输入框等支持 ValuePattern 的元素写入文本(不改剪贴板、不模拟键盘)。",
            json!({
                "type": "object",
                "properties": {
                    "element_id": { "type": "string" },
                    "value": { "type": "string", "description": "要写入的文本" }
                },
                "required": ["element_id", "value"]
            }),
        ),
        tool(
            "ui_type_text",
            "把窗口切到前台后模拟键盘输入文本(支持中文)。仅在元素不支持 ValuePattern 时使用。",
            json!({
                "type": "object",
                "properties": {
                    "hwnd": { "type": "number", "description": "目标窗口句柄" },
                    "text": { "type": "string", "description": "要输入的文本" }
                },
                "required": ["hwnd", "text"]
            }),
        ),
        tool(
            "ui_focus_window",
            "把目标窗口切到前台(最小化会先恢复)。",
            json!({
                "type": "object",
                "properties": { "hwnd": { "type": "number" } },
                "required": ["hwnd"]
            }),
        ),
    ]
}

fn tool(name: &str, desc: &str, params: Value) -> ToolDef {
    ToolDef {
        r#type: "function".into(),
        function: ToolFunctionDef {
            name: name.into(),
            description: Some(desc.into()),
            parameters: Some(params),
        },
    }
}

/// 执行入口:返回 (给模型的内容, 是否成功)。
pub fn execute(name: &str, args: &Value) -> (String, bool) {
    match run(name, args) {
        Ok(s) => (s, true),
        Err(e) => (e, false),
    }
}

fn run(name: &str, args: &Value) -> Result<String, String> {
    // 总开关:GUI 自动化已并入「完全信任」审批模式;非完全信任一律拒绝,
    // 进程白名单 / 句柄 TTL / 字数上限等内容层约束在完全信任下仍然生效。
    let approval = crate::mcp::approval::load_policy(&crate::paths::data_dir());
    if approval.mode != crate::mcp::approval::ApprovalMode::Full {
        return Err(
            "GUI 自动化已并入完全信任模式:请在 设置 → MCP 工具 中将审批模式切为「完全信任」后使用"
                .into(),
        );
    }
    let policy = policy::load(&crate::paths::data_dir());

    match name {
        "ui_list_windows" => {
            let windows = uia::list_windows();
            if windows.is_empty() {
                return Ok("未发现可见窗口(可能全部最小化或没有打开的应用)".into());
            }
            Ok(windows
                .iter()
                .map(|w| format!("hwnd={} process={} title={}", w.hwnd, w.process, w.title))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "ui_find_elements" => {
            let hwnd = num_arg(args, "hwnd")?;
            let window = uia::find_window(hwnd)
                .ok_or_else(|| format!("窗口 {hwnd} 不存在或不可见,请重新执行 ui_list_windows"))?;
            policy.ensure_app(&window.process)?;
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| (n as usize).clamp(1, 200))
                .unwrap_or(policy.max_elements());
            let query = uia::ElementQuery {
                name: str_arg(args, "name"),
                automation_id: str_arg(args, "automation_id"),
                control_type: str_arg(args, "control_type"),
            };
            let found = uia::find_elements(hwnd, &query, limit)?;
            if found.is_empty() {
                return Ok("未找到匹配元素;可放宽 name / control_type 条件后重试".into());
            }
            Ok(found
                .iter()
                .map(|e| {
                    format!(
                        "element_id={} type={} name={} automation_id={} rect={},{},{},{} enabled={}",
                        e.element_id,
                        e.control_type,
                        e.name,
                        e.automation_id,
                        e.rect[0],
                        e.rect[1],
                        e.rect[2],
                        e.rect[3],
                        e.enabled
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "ui_invoke" => {
            let el = element_arg(&policy, args)?;
            let how = uia::invoke_element(&el)?;
            Ok(format!("{} `{}`", how, el.name))
        }
        "ui_set_value" => {
            let el = element_arg(&policy, args)?;
            let value = args
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "缺少 value 参数".to_string())?;
            if value.chars().count() > policy.max_text_len() {
                return Err(format!(
                    "value 超过长度上限({} 字符)",
                    policy.max_text_len()
                ));
            }
            let how = uia::set_value(&el, value)?;
            Ok(format!("{} `{}`", how, el.name))
        }
        "ui_type_text" => {
            let hwnd = num_arg(args, "hwnd")?;
            let window =
                uia::find_window(hwnd).ok_or_else(|| format!("窗口 {hwnd} 不存在或不可见"))?;
            policy.ensure_app(&window.process)?;
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "缺少 text 参数".to_string())?;
            let max = policy.max_text_len();
            let text = if text.chars().count() > max {
                text.chars().take(max).collect::<String>()
            } else {
                text.to_string()
            };
            super::input::focus_window(hwnd)?;
            super::input::type_unicode(&text)?;
            Ok(format!(
                "已向 `{}` 输入 {} 个字符",
                window.title,
                text.chars().count()
            ))
        }
        "ui_focus_window" => {
            let hwnd = num_arg(args, "hwnd")?;
            let window =
                uia::find_window(hwnd).ok_or_else(|| format!("窗口 {hwnd} 不存在或不可见"))?;
            policy.ensure_app(&window.process)?;
            super::input::focus_window(hwnd)?;
            Ok(format!("已切换到 `{}`", window.title))
        }
        _ => Err(format!("未知 GUI 工具 `{name}`")),
    }
}

/// 取 hwnd 参数(兼容数字与数字字符串)。
fn num_arg(args: &Value, key: &str) -> Result<i64, String> {
    match args.get(key) {
        Some(v) => v
            .as_i64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
            .ok_or_else(|| format!("参数 `{key}` 必须是整数句柄")),
        None => Err(format!("缺少 `{key}` 参数")),
    }
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 从注册表取出元素并复核进程白名单。
fn element_arg(policy: &policy::GuiPolicy, args: &Value) -> Result<super::ElementInfo, String> {
    let id = str_arg(args, "element_id").ok_or_else(|| "缺少 element_id 参数".to_string())?;
    let el = REGISTRY.get(&id, policy.ttl_secs()).ok_or_else(|| {
        "element_id 已失效(过期或窗口已刷新),请重新执行 ui_find_elements".to_string()
    })?;
    policy.ensure_app(&el.process)?;
    Ok(el)
}
