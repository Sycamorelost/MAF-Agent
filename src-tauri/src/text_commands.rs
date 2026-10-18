//! AI 指令库(迭代 B3 / F1):内置指令 + 用户自定义,落盘 `data/commands.json`。
//!
//! 指令 = { id, name, icon, prompt_template, builtin, sort, scope }:
//! - scope:`chat`(对话指令面板)/ `both`(同 chat;`selection` 为历史遗留值,当前无消费方);
//! - 模板变量:`{selection}`(划词文本,调用方传入)、`{clipboard}`(剪贴板,后端 FFI 读取);
//! - 空模板 = 只把选中文本作为引用带入对话,由用户自行提问。
//!
//! 迁移:commands.json 不存在时,把旧版 settings.json `selection_ask_buttons` 中
//! 用户自定义按钮(id 不在旧默认 ask/translate/summary/code 内)转换为自定义指令并落盘。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::memory::validate_id;
use crate::selection;

/// 文本指令(内置与自定义共用同一结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextCommand {
    pub id: String,
    pub name: String,
    /// 图标 key,前端映射到线性 SVG(禁 emoji)
    pub icon: String,
    /// 指令模板;支持 {selection} / {clipboard} 变量;空 = 只带入引用
    pub prompt_template: String,
    /// 是否内置(内置不可删除/覆盖);JSON 中缺省 false
    #[serde(default)]
    pub builtin: bool,
    /// 排序权重(小的在前)
    #[serde(default)]
    pub sort: i32,
    /// selection / chat / both
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "both".into()
}

/// 合法 scope。
pub const SCOPES: [&str; 3] = ["selection", "chat", "both"];

/// 9 条内置指令:问 AI / 总结 / 解释 / 翻译中 / 翻译英 / 润色 / 纠错 / 改正式 / 解释代码。
pub fn builtin_commands() -> Vec<TextCommand> {
    let t = |id: &str, name: &str, icon: &str, sort: i32, scope: &str, template: &str| TextCommand {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        prompt_template: template.into(),
        builtin: true,
        sort,
        scope: scope.into(),
    };
    vec![
        t("summary", "总结", "file-text", 20, "both", "请总结以下内容:\n\n{selection}"),
        t("explain", "解释", "search", 30, "both", "请解释以下内容:\n\n{selection}"),
        t(
            "translate-zh",
            "翻译为中文",
            "languages",
            40,
            "both",
            "请把以下内容翻译成中文:\n\n{selection}",
        ),
        t(
            "translate-en",
            "翻译为英文",
            "languages",
            50,
            "both",
            "请把以下内容翻译成英文:\n\n{selection}",
        ),
        t(
            "polish",
            "润色",
            "wand-sparkles",
            60,
            "both",
            "请润色以下文本,保持原意,提升表达的流畅度与专业性:\n\n{selection}",
        ),
        t(
            "fix-grammar",
            "纠错",
            "shield-check",
            70,
            "both",
            "请检查并纠正以下文本中的错别字与语法错误,输出纠正后的文本并列出主要修改点:\n\n{selection}",
        ),
        t(
            "formalize",
            "改正式",
            "clipboard-list",
            80,
            "both",
            "请把以下内容改写得更正式、得体:\n\n{selection}",
        ),
        t(
            "explain-code",
            "解释代码",
            "bug",
            90,
            "both",
            "请解释以下代码的功能与关键逻辑:\n\n{selection}",
        ),
    ]
}

fn custom_path() -> PathBuf {
    crate::paths::data_dir().join("commands.json")
}

/// 旧版划词按钮(settings.json `selection_ask_buttons`),仅迁移期使用。
#[derive(Debug, Deserialize)]
struct LegacyButton {
    id: String,
    label: String,
    #[serde(default)]
    prompt: String,
}

#[derive(Debug, Default, Deserialize)]
struct LegacySettings {
    #[serde(default)]
    selection_ask_buttons: Vec<LegacyButton>,
}

/// 旧默认按钮 id(这些已被内置指令覆盖,不参与迁移)。
const LEGACY_DEFAULT_IDS: [&str; 4] = ["ask", "translate", "summary", "code"];

/// 读取用户自定义指令;文件缺失时尝试从旧版划词按钮迁移,损坏时返回空(本地优先不报错)。
fn load_custom() -> Vec<TextCommand> {
    let p = custom_path();
    match fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => migrate_legacy_buttons(),
    }
}

/// 一次性迁移:旧版自定义划词按钮 → 自定义指令(scope=selection,模板原样保留)。
fn migrate_legacy_buttons() -> Vec<TextCommand> {
    let Ok(text) = fs::read_to_string(crate::paths::data_dir().join("settings.json")) else {
        return Vec::new();
    };
    let legacy: LegacySettings = serde_json::from_str(&text).unwrap_or_default();
    let mut out = Vec::new();
    for (i, b) in legacy.selection_ask_buttons.into_iter().enumerate() {
        if LEGACY_DEFAULT_IDS.contains(&b.id.as_str()) {
            continue;
        }
        out.push(TextCommand {
            id: b.id,
            name: b.label,
            icon: "sparkles".into(),
            prompt_template: b.prompt,
            builtin: false,
            sort: 1000 + i as i32,
            scope: "selection".into(),
        });
    }
    if !out.is_empty() {
        // 落盘失败不影响本次使用(下次启动重试迁移)
        let _ = save_custom(&out);
    }
    out
}

fn save_custom(list: &[TextCommand]) -> Result<(), String> {
    let p = custom_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 全部指令 = 内置 + 自定义(自定义 id 与内置冲突时忽略自定义,防覆盖),按 sort 排序。
pub fn list_all() -> Vec<TextCommand> {
    let builtins = builtin_commands();
    let ids: Vec<String> = builtins.iter().map(|t| t.id.clone()).collect();
    let mut out = builtins;
    for mut c in load_custom() {
        if ids.contains(&c.id) {
            continue;
        }
        c.builtin = false;
        out.push(c);
    }
    out.sort_by_key(|c| c.sort);
    out
}

/// 按 id 查指令(内置优先)。
pub fn find(id: &str) -> Option<TextCommand> {
    list_all().into_iter().find(|t| t.id == id)
}

/// 新增/更新自定义指令。校验:id 合法(白名单字符)、name 非空、scope 合法、不覆盖内置。
pub fn upsert_custom(cmd: TextCommand) -> Result<(), String> {
    validate_id(&cmd.id).map_err(|_| format!("非法指令 id:{}", cmd.id))?;
    if cmd.name.trim().is_empty() {
        return Err("指令名称不能为空".into());
    }
    if !SCOPES.contains(&cmd.scope.as_str()) {
        return Err(format!("非法 scope:{}(可选:{})", cmd.scope, SCOPES.join("/")));
    }
    if builtin_commands().iter().any(|b| b.id == cmd.id) {
        return Err(format!("不能覆盖内置指令:{}", cmd.id));
    }
    let mut list = load_custom();
    let mut cmd = cmd;
    cmd.builtin = false;
    if let Some(x) = list.iter_mut().find(|x| x.id == cmd.id) {
        *x = cmd;
    } else {
        list.push(cmd);
    }
    save_custom(&list)
}

/// 删除自定义指令;内置指令拒绝删除。
pub fn remove_custom(id: &str) -> Result<(), String> {
    if builtin_commands().iter().any(|b| b.id == id) {
        return Err(format!("内置指令不可删除:{id}"));
    }
    let list = load_custom();
    let next: Vec<_> = list.into_iter().filter(|t| t.id != id).collect();
    save_custom(&next)
}

/// 渲染模板(纯函数,便于测试):{selection} 用传入文本替换(缺省空串),
/// {clipboard} 用给定剪贴板文本替换;结尾空白清理。
pub fn render_template(template: &str, selection: Option<&str>, clipboard: &str) -> String {
    let mut out = template.to_string();
    if out.contains("{selection}") {
        out = out.replace("{selection}", selection.unwrap_or(""));
    }
    if out.contains("{clipboard}") {
        out = out.replace("{clipboard}", clipboard.trim());
    }
    out.trim_end().to_string()
}

/// 按 id 渲染指令:{clipboard} 变量在此经 FFI 读取剪贴板。
pub fn render(id: &str, selection: Option<&str>) -> Result<String, String> {
    let cmd = find(id).ok_or_else(|| format!("指令不存在:{id}"))?;
    let clip = if cmd.prompt_template.contains("{clipboard}") {
        selection::read_clipboard_text().unwrap_or_default()
    } else {
        String::new()
    };
    Ok(render_template(&cmd.prompt_template, selection, &clip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_commands_valid_and_unique() {
        let list = builtin_commands();
        assert!(list.len() >= 6, "内置指令至少 6 条");
        let mut ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), list.len(), "指令 id 必须唯一");
        for t in &list {
            assert!(SCOPES.contains(&t.scope.as_str()), "非法 scope:{}", t.scope);
            assert!(t.builtin);
            assert!(!t.name.trim().is_empty(), "{} 缺 name", t.id);
            assert!(validate_id(&t.id).is_ok(), "{} id 非法", t.id);
        }
    }

    #[test]
    fn render_template_substitutes_selection() {
        let out = render_template("请总结:\n\n{selection}", Some("你好世界"), "");
        assert_eq!(out, "请总结:\n\n你好世界");
    }

    #[test]
    fn render_template_empty_selection_trims_tail() {
        // 对话场景无 selection:变量清空并清理结尾空行,得到可用的提示语
        let out = render_template("请总结:\n\n{selection}", None, "");
        assert_eq!(out, "请总结:");
    }

    #[test]
    fn render_template_substitutes_clipboard() {
        let out = render_template("翻译:{clipboard}", None, "  hello  ");
        assert_eq!(out, "翻译:hello");
    }

    #[test]
    fn render_template_without_variables_unchanged() {
        let out = render_template("直接指令", None, "ignored");
        assert_eq!(out, "直接指令");
    }

    #[test]
    fn upsert_rejects_builtin_id() {
        let cmd = TextCommand {
            id: "summary".into(),
            name: "x".into(),
            icon: "sparkles".into(),
            prompt_template: "y".into(),
            builtin: false,
            sort: 1,
            scope: "both".into(),
        };
        let err = upsert_custom(cmd).unwrap_err();
        assert!(err.contains("内置"), "应拒绝覆盖内置:{err}");
    }

    #[test]
    fn upsert_rejects_bad_scope_and_id() {
        let base = TextCommand {
            id: "ok_id".into(),
            name: "x".into(),
            icon: "sparkles".into(),
            prompt_template: "y".into(),
            builtin: false,
            sort: 1,
            scope: "everywhere".into(),
        };
        assert!(upsert_custom(base.clone()).unwrap_err().contains("scope"));
        let bad = TextCommand { id: "../evil".into(), scope: "chat".into(), ..base };
        assert!(upsert_custom(bad).unwrap_err().contains("非法指令 id"));
    }

    #[test]
    fn remove_rejects_builtin_id() {
        assert!(remove_custom("summary").unwrap_err().contains("内置"));
    }

    /// 旧版/缺字段 JSON 能解析为默认(本地优先不报错)。
    #[test]
    fn custom_json_missing_fields_default() {
        let back: Vec<TextCommand> = serde_json::from_str(
            r#"[{"id":"x","name":"x","icon":"sparkles","prompt_template":"p"}]"#,
        )
        .unwrap();
        assert!(!back[0].builtin);
        assert_eq!(back[0].sort, 0);
        assert_eq!(back[0].scope, "both");
    }
}
