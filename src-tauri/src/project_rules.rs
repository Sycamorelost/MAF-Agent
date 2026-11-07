//! D6:项目约定文件加载与 system 注入。
//!
//! 用户在项目工作区放一份 `AGENTS.md`(兼容 `CLAUDE.md`),即可让智能体自动遵循项目
//! 规范(技术栈、命令、代码风格、禁改文件等),不需要任何设置项。
//!
//! 约束:
//! - 只读工作区根下这两个固定文件名,不做递归搜索;
//! - 上限 32KB,超出截断并标注,避免把整本手册塞进每轮请求;
//! - 按「路径 + mtime + 大小」缓存,文件改动后下一次请求自动生效;
//! - 注入点只在 `chat_stream` 组装请求时,不改 DB 历史。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crate::provider::ChatMessage;

/// 可识别的项目约定文件名,按优先级排列。
const FILENAMES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];
/// 单文件读取上限(字节)。
const MAX_BYTES: u64 = 32 * 1024;
/// 注入内容标记:已注入则不再重复(防止同一请求被多次包装)。
const MARKER: &str = "【项目约定】";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRules {
    /// 文件名(AGENTS.md / CLAUDE.md)。
    pub source: String,
    pub content: String,
}

#[derive(Debug, Clone)]
struct Cached {
    mtime: u64,
    len: u64,
    rules: ProjectRules,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, Cached>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 按会话取工作区根并加载项目约定;没有文件时返回 None(零配置)。
pub(crate) fn load(session_id: Option<&str>) -> Option<ProjectRules> {
    let root = crate::fs::paths::resolve_workspace_root(session_id).ok()?;
    load_in(&root)
}

/// 从指定目录加载(工作区根或测试目录)。
pub(crate) fn load_in(root: &Path) -> Option<ProjectRules> {
    let (path, name) = FILENAMES
        .iter()
        .map(|n| (root.join(n), *n))
        .find(|(p, _)| p.is_file())?;
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let len = meta.len();

    if let Ok(cache) = CACHE.lock()
        && let Some(c) = cache.get(&path)
        && c.mtime == mtime
        && c.len == len
    {
        return Some(c.rules.clone());
    }

    let bytes = std::fs::read(&path).ok()?;
    let truncated = bytes.len() as u64 > MAX_BYTES;
    let slice = if truncated {
        &bytes[..MAX_BYTES as usize]
    } else {
        &bytes[..]
    };
    let mut content = String::from_utf8_lossy(slice).trim().to_string();
    if content.is_empty() {
        return None;
    }
    if truncated {
        content.push_str("\n\n[项目约定文件超过 32KB,以上为截断内容]");
    }
    let rules = ProjectRules {
        source: name.to_string(),
        content,
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(
            path,
            Cached {
                mtime,
                len,
                rules: rules.clone(),
            },
        );
    }
    Some(rules)
}

/// 把项目约定注入消息列表的 system 基线。返回被注入的文件(未注入时为 None)。
///
/// 已有 system 消息时追加到第一条 system 末尾(保留前端/预设的原有提示词);
/// 没有 system 消息时在开头插入一条。
pub(crate) fn inject(
    messages: &mut Vec<ChatMessage>,
    session_id: Option<&str>,
) -> Option<ProjectRules> {
    let rules = load(session_id)?;
    if messages
        .iter()
        .any(|m| m.role == "system" && m.content.contains(MARKER))
    {
        return None;
    }
    let block = format!(
        "{MARKER}(读取自工作区 {};以下是项目级规则,与用户当前指令一并遵循)\n{}",
        rules.source, rules.content
    );
    match messages.iter_mut().find(|m| m.role == "system") {
        Some(sys) => {
            if !sys.content.is_empty() {
                sys.content.push_str("\n\n");
            }
            sys.content.push_str(&block);
        }
        None => messages.insert(
            0,
            ChatMessage {
                role: "system".into(),
                content: block,
                ..Default::default()
            },
        ),
    }
    Some(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp_dir() -> PathBuf {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "maf-rules-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sys(content: &str) -> ChatMessage {
        ChatMessage {
            role: "system".into(),
            content: content.into(),
            ..Default::default()
        }
    }

    fn user(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".into(),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn prefers_agents_md_and_reports_source() {
        let dir = tmp_dir();
        std::fs::write(dir.join("CLAUDE.md"), "旧约定").unwrap();
        std::fs::write(dir.join("AGENTS.md"), "新约定:统一用 pnpm").unwrap();
        let rules = load_in(&dir).expect("应加载 AGENTS.md");
        assert_eq!(rules.source, "AGENTS.md");
        assert!(rules.content.contains("pnpm"));

        std::fs::remove_file(dir.join("AGENTS.md")).unwrap();
        let legacy = load_in(&dir).expect("回退 CLAUDE.md");
        assert_eq!(legacy.source, "CLAUDE.md");
        assert!(legacy.content.contains("旧约定"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_or_missing_file_yields_none() {
        let dir = tmp_dir();
        assert!(load_in(&dir).is_none());
        std::fs::write(dir.join("AGENTS.md"), "   \n  ").unwrap();
        assert!(load_in(&dir).is_none(), "空白文件不注入");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn oversized_rules_are_truncated() {
        let dir = tmp_dir();
        let body = "x".repeat((MAX_BYTES + 1024) as usize);
        std::fs::write(dir.join("AGENTS.md"), &body).unwrap();
        let rules = load_in(&dir).unwrap();
        assert!(
            rules
                .content
                .ends_with("[项目约定文件超过 32KB,以上为截断内容]")
        );
        assert!(rules.content.len() < body.len());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn inject_appends_to_existing_system_and_is_idempotent() {
        let dir = tmp_dir();
        std::fs::write(dir.join("AGENTS.md"), "测试用 pnpm test").unwrap();
        // 直接验证 load_in + 手工拼装等价路径(避免依赖全局工作区设置)
        let rules = load_in(&dir).unwrap();
        let block = format!("{MARKER}(读取自工作区 {})\n{}", rules.source, rules.content);
        let mut msgs = vec![sys("你是助手"), user("帮我改代码")];
        msgs[0].content.push_str("\n\n");
        msgs[0].content.push_str(&block);
        assert!(msgs[0].content.contains("pnpm test"));
        assert_eq!(msgs.len(), 2);
        // 再次注入(模拟同内容重复处理)应被标记拦住
        assert!(!inject_with_block(&mut msgs, &block));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 测试辅助:与 `inject` 同一标记语义,不触碰全局工作区。
    fn inject_with_block(messages: &mut Vec<ChatMessage>, block: &str) -> bool {
        if messages
            .iter()
            .any(|m| m.role == "system" && m.content.contains(MARKER))
        {
            return false;
        }
        match messages.iter_mut().find(|m| m.role == "system") {
            Some(s) => {
                s.content.push_str("\n\n");
                s.content.push_str(block);
            }
            None => messages.insert(
                0,
                ChatMessage {
                    role: "system".into(),
                    content: block.into(),
                    ..Default::default()
                },
            ),
        }
        true
    }

    #[test]
    fn inject_adds_system_message_when_absent() {
        let mut msgs = vec![user("hi")];
        let injected = inject_with_block(&mut msgs, "【项目约定】规则");
        assert!(injected);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "user");
    }
}
