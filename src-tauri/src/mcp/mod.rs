//! MCP 模块入口。
//!
//! Phase 4 先搭建配置 + 安全层骨架;Phase 5 追加 rmcp stdio 运行时 + ChatStream 集成。

pub mod approval;
pub mod builtin;
pub mod capability;
pub mod config;
pub mod dispatcher;
pub mod http;
pub mod runtime;
pub mod safety;
pub mod shell;

// Phase 4 骨架导出;safety 四个函数/常量 Phase 5e/6 ChatStream/shell 才真正消费
#[allow(unused_imports)]
pub use approval::{
    APPROVAL_TIMEOUT_SECS, ApprovalMode, ApprovalPolicy, ApprovalRequest, ApprovalResult,
    ApprovalRisk, ApprovalStore,
};
#[allow(unused_imports)]
pub use capability::{Capability, CapabilityStore, Scope};
#[allow(unused_imports)]
pub use config::{McpConfig, McpServerConfig, Transport, default_servers};
pub use runtime::{McpRuntime, McpToolCall, McpToolInfo, McpToolResult};
#[allow(unused_imports)]
pub use safety::{resolve_safe_path, truncate_chars, validate_command, validate_server_command};

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

/// 全局 MCP 配置 —— 用 RwLock 包一层,前端调用时读写不阻塞。
pub static GLOBAL_CONFIG: LazyLock<RwLock<McpConfig>> =
    LazyLock::new(|| RwLock::new(McpConfig::default()));

/// 配置文件路径 —— 放在 data 目录下,与 settings.json 同位置。
pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-servers.json")
}

/// 从磁盘加载配置(文件不存在则写入 default 并返回)。
/// 旧配置会按 `presets_version` 补齐新增的预置 server 一次(已存在同名则跳过)。
/// D8:加载后解密各 server 的 custom headers(明文旧值自动兼容,不加密的直接透传)。
pub fn load_or_init(data_dir: &Path) -> McpConfig {
    let path = config_path(data_dir);
    if path.exists() {
        let mut cfg = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<McpConfig>(&s).ok())
            .unwrap_or_else(default_servers);
        decrypt_headers(&mut cfg);
        if merge_presets(&mut cfg) {
            // merge_presets 可能改写配置,落盘前重新加密
            let mut to_save = cfg.clone();
            encrypt_headers(&mut to_save);
            let _ = std::fs::write(
                &path,
                serde_json::to_string_pretty(&to_save).unwrap_or_default(),
            );
        }
        cfg
    } else {
        let def = default_servers();
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&def).unwrap_or_default(),
        );
        def
    }
}

/// 加密所有 server 的 header 值(DPAPI/AES-GCM),带 `enc:` 前缀。
/// 仅加密非空值;空串保持不变。
fn encrypt_headers(cfg: &mut McpConfig) {
    for s in &mut cfg.servers {
        for v in s.headers.values_mut() {
            if v.is_empty() || v.starts_with("enc:") {
                continue;
            }
            if let Ok(enc) = crate::provider::keychain::encrypt_value(v) {
                *v = enc;
            }
        }
    }
}

/// 解密所有 server 的 header 值。`enc:` 前缀的解密,其余(明文旧值)原样保留。
fn decrypt_headers(cfg: &mut McpConfig) {
    for s in &mut cfg.servers {
        for v in s.headers.values_mut() {
            if !v.starts_with("enc:") {
                continue;
            }
            if let Ok(plain) = crate::provider::keychain::decrypt_value(v) {
                *v = plain;
            }
        }
    }
}

/// 把新增的预置 server 补进配置;返回是否发生变更。
/// v1→v2:开箱即用迁移 —— 已存在的预置 server 一次性置为启用(用户之后仍可手动关闭)。
fn merge_presets(cfg: &mut McpConfig) -> bool {
    if cfg.presets_version >= config::PRESETS_VERSION {
        return false;
    }
    let upgrading_to_v2 = cfg.presets_version < 2;
    let upgrading_to_v3 = cfg.presets_version < 3;
    for preset in config::preset_servers() {
        match cfg.servers.iter_mut().find(|s| s.name == preset.name) {
            Some(existing) => {
                if upgrading_to_v2 {
                    existing.enabled = true;
                }
            }
            None => cfg.servers.push(preset),
        }
    }
    // v3:开箱即用 —— 旧配置里显式存了 builtin_tools: false,一次性点亮(用户仍可手动关闭)
    if upgrading_to_v3 {
        cfg.builtin_tools = true;
    }
    cfg.presets_version = config::PRESETS_VERSION;
    true
}

/// 保存配置到磁盘 + 更新全局缓存。
/// D8:落盘前加密所有 server 的 custom headers(DPAPI/AES-GCM),内存中保持明文便于运行时使用。
pub fn save(data_dir: &Path, cfg: &McpConfig) -> Result<(), String> {
    let path = config_path(data_dir);
    let mut to_save = cfg.clone();
    encrypt_headers(&mut to_save);
    let json = serde_json::to_string_pretty(&to_save).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入失败: {e}"))?;
    if let Ok(mut g) = GLOBAL_CONFIG.write() {
        *g = cfg.clone();
    }
    Ok(())
}

/// 单个审计日志文件大小上限(8 MB)。超过后按 [`AUDIT_BACKUPS`] 份滚动保留。
/// SEC-3:避免长期运行后日志无限增长占满磁盘(审计写入是高频路径)。
const MAX_AUDIT_BYTES: u64 = 8 * 1024 * 1024;
/// 轮转保留的历史分卷数(不含当前文件)。
const AUDIT_BACKUPS: usize = 3;

/// 分卷路径:`mcp-audit.jsonl` → `mcp-audit.jsonl.1`
fn audit_backup_path(file: &Path, index: usize) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{index}"));
    file.with_file_name(name)
}

/// 按大小轮转:当前文件达到上限时滚到 `.1`,旧分卷依次后移,最旧的丢弃。
fn rotate_audit_if_needed(file: &Path) {
    let Ok(meta) = std::fs::metadata(file) else {
        return;
    };
    if meta.len() < MAX_AUDIT_BYTES {
        return;
    }
    // 先删最旧分卷,为整条 rename 链腾位
    let _ = std::fs::remove_file(audit_backup_path(file, AUDIT_BACKUPS));
    for i in (1..AUDIT_BACKUPS).rev() {
        let from = audit_backup_path(file, i);
        let to = audit_backup_path(file, i + 1);
        let _ = std::fs::rename(&from, &to);
    }
    if let Err(e) = std::fs::rename(file, audit_backup_path(file, 1)) {
        log::warn!("rotate mcp audit log failed: {e}");
    }
}

/// 审计日志单行 JSONL —— 每次工具调用前后各写一行。
/// 不依赖第三方 crate,手动 serde_json + writeln。
///
/// SEC-7:`detail` 在落盘前经过 [`sanitize_detail`] 脱敏,
/// 避免命令行参数中的 `sk-*`、`Bearer`、`api_key=...` 等密钥明文写入审计日志。
/// SEC-3:写入前按 [`MAX_AUDIT_BYTES`] 轮转;写入失败不再静默丢弃,改为 `log::warn!` 告警。
pub fn audit(data_dir: &Path, event: &str, server: &str, tool: &str, detail: &str) {
    let dir = data_dir.join("logs");
    let file = dir.join("mcp-audit.jsonl");
    if let Some(parent) = file.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("create audit log dir failed: {e}");
    }
    let safe_detail = sanitize_detail(detail);
    let line = serde_json::json!({
        "ts": chrono::Local::now().to_rfc3339(),
        "event": event,
        "server": server,
        "tool": tool,
        "detail": safe_detail,
    });
    rotate_audit_if_needed(&file);
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", line).map_err(|e| std::io::Error::other(e.to_string()))
        })
    {
        log::warn!("write mcp audit log failed: {e}");
    }
}

/// 审计日志脱敏:替换 detail 中疑似密钥/Token/密码的片段。
///
/// 覆盖的典型形态:
/// - `sk-xxxxxxxxxxxx`(OpenAI / DeepSeek / Moonshot 风格)
/// - `ghp_xxxxxxxx` / `gho_` / `ghu_` / `ghs_` / `ghr_`(GitHub Token)
/// - `Bearer <token>`(HTTP  Authorization 头)
/// - `api_key=...` / `password=...` / `token=...` 等键值对(保留 key,掩码 value)
///
/// 启发式规则,不保证 100% 覆盖;目标是拦截最常见的明文泄漏。
fn sanitize_detail(detail: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    // 整体替换为 *** 的 token 形态
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    let token_re = TOKEN_RE.get_or_init(|| {
        Regex::new(r#"(sk-[A-Za-z0-9_-]{12,})|(gh[pousr]_[A-Za-z0-9]{20,})|((?i)bearer\s+[A-Za-z0-9._\-]{8,})"#)
            .expect("audit token regex")
    });
    let s = token_re.replace_all(detail, "***").to_string();

    // key=value / key: value 形态:保留 key 与分隔符,掩码 value
    static KV_RE: OnceLock<Regex> = OnceLock::new();
    let kv_re = KV_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(api[_-]?key|apikey|secret[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|passwd|pwd|authorization|auth)([=:"'\s]+)([^\s"'&]{4,})"#,
        )
        .expect("audit kv regex")
    });
    kv_re
        .replace_all(&s, |caps: &regex::Captures| {
            format!("{}{}***", &caps[1], &caps[2])
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧配置升级:补齐缺失的预置项,且不覆盖用户已改过的同名项。
    #[test]
    fn merge_presets_keeps_user_entry_and_adds_missing() {
        let mut cfg = McpConfig::default();
        let mut mine = config::preset_servers().remove(0);
        mine.enabled = true; // 用户已启用,合并时不能被默认值重置
        cfg.servers.push(mine);

        assert!(merge_presets(&mut cfg));
        assert_eq!(cfg.servers.len(), config::preset_servers().len());
        assert_eq!(cfg.presets_version, config::PRESETS_VERSION);
        let first = cfg.servers.iter().find(|s| s.name == "项目文件").unwrap();
        assert!(first.enabled, "用户已启用的项不应被覆盖");

        // 版本已是最新 → 不再重复合并
        assert!(!merge_presets(&mut cfg));
    }

    /// SEC-7 回归:审计 detail 中的密钥必须被掩码,普通字段保持原样。
    #[test]
    fn sanitize_detail_masks_secrets_keeps_plain() {
        // 普通审计字段不应被误伤
        let plain = "ok=true exit=0 ms=123 line=git status cwd=C:\\proj";
        assert_eq!(sanitize_detail(plain), plain);

        // sk- 风格密钥
        assert_eq!(
            sanitize_detail("call with sk-abcdefghijklmnop ok"),
            "call with *** ok"
        );

        // GitHub token
        assert_eq!(
            sanitize_detail("token ghp_abcdefghijklmnopqrstuvwxyz0123456789 end"),
            "token *** end"
        );

        // Bearer token
        assert_eq!(
            sanitize_detail("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 payload"),
            "Authorization: *** payload"
        );

        // key=value 形态:保留 key,掩码 value
        assert_eq!(
            sanitize_detail("api_key=sk-secret-value-here mode=full"),
            "api_key=*** mode=full"
        );
        assert_eq!(
            sanitize_detail("password: hunter2hunter2 done"),
            "password: *** done"
        );

        // 太短的 sk- 前缀不掩码(避免误伤 sk-1 之类的短标识)
        assert_eq!(sanitize_detail("flag=sk-123"), "flag=sk-123");
    }

    /// SEC-3:超过上限的文件应被轮转为 `.1`,新文件可继续写入。
    #[test]
    fn audit_rotates_when_exceeding_max_size() {
        let dir = std::env::temp_dir().join(format!("maf-audit-{}", uuid::Uuid::new_v4()));
        // 与 audit() 一致:文件位于 <data_dir>/logs/ 下
        let logs = dir.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let file = logs.join("mcp-audit.jsonl");
        // 造一个刚好超过上限的文件
        std::fs::write(&file, vec![b'x'; (MAX_AUDIT_BYTES + 1) as usize]).unwrap();

        rotate_audit_if_needed(&file);
        assert!(!file.exists(), "超限文件应被轮转走");
        assert!(audit_backup_path(&file, 1).exists(), "轮转后应存在 .1 分卷");

        // 新文件可正常写入
        audit(&dir, "call_tool", "srv", "tool", "ok");
        assert!(file.exists(), "轮转后应能重新创建日志文件");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// SEC-3:未达上限时不得轮转,否则会丢日志。
    #[test]
    fn audit_keeps_file_when_under_limit() {
        let dir = std::env::temp_dir().join(format!("maf-audit-{}", uuid::Uuid::new_v4()));
        let logs = dir.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let file = logs.join("mcp-audit.jsonl");
        std::fs::write(&file, b"small").unwrap();

        rotate_audit_if_needed(&file);
        assert!(file.exists(), "未超限文件不应被轮转");
        assert!(!audit_backup_path(&file, 1).exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
