//! GUI 自动化策略 —— 持久化在 `data/gui-policy.json`。
//!
//! 总开关已并入审批策略「完全信任」(见 gui::tools::run);本文件只保留
//! 内容层约束:进程白名单(默认只放行记事本)、句柄 TTL、返回上限、字数上限。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 元素句柄默认存活时间(秒)。
pub const DEFAULT_TTL_SECS: i64 = 600;
/// 单次查找默认返回上限。
pub const DEFAULT_MAX_ELEMENTS: usize = 20;
/// 单次输入默认字符上限。
pub const DEFAULT_MAX_TEXT: usize = 2000;

/// GUI 自动化策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiPolicy {
    /// 已废弃:总开关由审批策略「完全信任」接管;字段保留仅为兼容旧配置文件
    pub enabled: bool,
    /// 允许操作的进程名(如 notepad.exe);`*` 表示不限制(不建议)
    pub allowed_apps: Vec<String>,
    /// 元素句柄存活时长(秒)
    #[serde(default = "default_ttl")]
    pub element_ttl_secs: i64,
    /// 单次查找最多返回的元素数
    #[serde(default = "default_max_elements")]
    pub max_elements: usize,
    /// 单次输入最多字符数
    #[serde(default = "default_max_text")]
    pub max_text_len: usize,
}

fn default_ttl() -> i64 {
    DEFAULT_TTL_SECS
}
fn default_max_elements() -> usize {
    DEFAULT_MAX_ELEMENTS
}
fn default_max_text() -> usize {
    DEFAULT_MAX_TEXT
}

impl Default for GuiPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_apps: vec!["notepad.exe".into()],
            element_ttl_secs: DEFAULT_TTL_SECS,
            max_elements: DEFAULT_MAX_ELEMENTS,
            max_text_len: DEFAULT_MAX_TEXT,
        }
    }
}

impl GuiPolicy {
    /// 进程白名单校验(进程名精确匹配、大小写不敏感)。
    pub fn ensure_app(&self, process: &str) -> Result<(), String> {
        if process.is_empty() {
            return Err("无法识别目标进程,已拒绝".into());
        }
        if self.allowed_apps.iter().any(|a| a.trim() == "*") {
            return Ok(());
        }
        if self
            .allowed_apps
            .iter()
            .any(|a| a.trim().eq_ignore_ascii_case(process))
        {
            Ok(())
        } else {
            Err(format!(
                "进程 `{process}` 不在 GUI 白名单内,已拒绝;如需操作请先在设置页加入白名单"
            ))
        }
    }

    pub fn ttl_secs(&self) -> i64 {
        self.element_ttl_secs.clamp(30, 3600)
    }

    pub fn max_elements(&self) -> usize {
        self.max_elements.clamp(1, 200)
    }

    pub fn max_text_len(&self) -> usize {
        self.max_text_len.clamp(1, 4000)
    }

    /// 落盘前收敛越界值,避免前端传入极端配置。
    pub fn normalized(&self) -> Self {
        Self {
            enabled: self.enabled,
            allowed_apps: self
                .allowed_apps
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            element_ttl_secs: self.ttl_secs(),
            max_elements: self.max_elements(),
            max_text_len: self.max_text_len(),
        }
    }
}

/// 策略文件路径。
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("gui-policy.json")
}

/// 读策略(缺失/损坏 → 默认关闭)。
pub fn load(data_dir: &Path) -> GuiPolicy {
    std::fs::read_to_string(path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<GuiPolicy>(&s).ok())
        .map(|p| p.normalized())
        .unwrap_or_default()
}

/// 写策略(自动收敛越界值)。
pub fn save(data_dir: &Path, policy: &GuiPolicy) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&policy.normalized())
        .map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(path(data_dir), json).map_err(|e| format!("写入失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_closed_and_narrow() {
        let p = GuiPolicy::default();
        assert!(!p.enabled, "默认配置仍为关闭态(总开关由完全信任接管)");
        assert!(p.ensure_app("notepad.exe").is_ok());
        assert!(
            p.ensure_app("NOTEPAD.EXE").is_ok(),
            "进程名匹配应大小写不敏感"
        );
        assert!(p.ensure_app("chrome.exe").is_err(), "未列入白名单必须拒绝");
    }

    #[test]
    fn wildcard_and_empty_process() {
        let mut p = GuiPolicy::default();
        p.enabled = true;
        p.allowed_apps = vec!["*".into()];
        assert!(p.ensure_app("anything.exe").is_ok());
        p.allowed_apps = vec!["notepad.exe".into()];
        assert!(p.ensure_app("").is_err(), "空进程名一律拒绝");
    }

    #[test]
    fn normalization_clamps_limits() {
        let mut p = GuiPolicy::default();
        p.element_ttl_secs = 99_999;
        p.max_elements = 0;
        p.max_text_len = 999_999;
        let n = p.normalized();
        assert_eq!(n.element_ttl_secs, 3600);
        assert_eq!(n.max_elements, 1);
        assert_eq!(n.max_text_len, 4000);
    }
}
