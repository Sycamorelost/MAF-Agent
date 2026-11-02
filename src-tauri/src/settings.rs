//! 应用级设置(迭代 D3.7,迭代 D9 扩展划词提问),落盘 `data/settings.json`。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// 应用设置。所有字段缺省 = 走默认行为。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// 提炼专用 provider_id(空 = 用当前会话模型)
    #[serde(default)]
    pub summarize_provider_id: Option<String>,
    /// 提炼专用模型(空 = 用当前会话模型)
    #[serde(default)]
    pub summarize_model: Option<String>,
    /// 划词提问:任意应用中连按两次 Ctrl+C 唤起主窗口
    #[serde(default = "default_true")]
    pub selection_ask_enabled: bool,
    /// 主窗口最小化时隐藏到托盘(默认关:最小化保留任务栏图标,点 × 才进托盘)
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// 导航菜单中隐藏的功能项(对话/设置始终显示;空 = 全部显示)
    #[serde(default)]
    pub hidden_nav_items: Vec<String>,
    /// 导航菜单的显示顺序(存功能项 key;未列出的项按内置顺序补在末尾)
    #[serde(default)]
    pub nav_order: Vec<String>,
    /// 对话上下文窗口:每轮携带的历史消息条数(0 = 全部;默认 24)
    #[serde(default = "default_context_window")]
    pub context_window_messages: u32,
    /// 全裸模式(R3 A1):true = system 不追加身份锚,仅保留 Base 物理契约;
    /// 人格全交智能体提示或用户自带人设的中转模型
    #[serde(default)]
    pub bare_mode: bool,
    /// 首次启动引导是否已完成(R3 首次安装向导)
    #[serde(default)]
    pub first_run_completed: bool,
    /// 用户自定义底层 Prompt(空 = 使用内置默认 BASE_PHYSICAL_CONTRACT)
    #[serde(default)]
    pub custom_base_prompt: Option<String>,
    /// 用户自定义应用名称(空 = 使用 "myAgent",仅影响窗口标题/托盘/快捷方式)
    #[serde(default)]
    pub custom_app_name: Option<String>,
    /// 用户自定义应用图标路径(空 = 使用内置图标,仅影响窗口/托盘/快捷方式)
    #[serde(default)]
    pub custom_app_icon: Option<String>,
    /// 用户自定义首页提示语(空 = 使用默认「myAgent | 向更美好的生活出发！」)
    #[serde(default)]
    pub home_prompt: Option<String>,
    /// 默认工作区根目录(空 = `<数据目录>/workspace`;环境变量 `MYAGENT_WORKSPACE` 仍优先)。
    /// 这是「相对路径」的基准目录;绝对路径不受其限制。
    #[serde(default)]
    pub workspace_root: Option<String>,
    /// 默认工作台模式(P3 / D16):chat = 普通会话;work = 专业 Agent 工作台。
    /// 决定首页落点与新建会话的界面形态;单个会话可用 sessions.workbench_mode 覆盖。
    #[serde(default = "default_workbench")]
    pub default_workbench: String,
    /// 全局默认运行时预设(P5 / D19):economy / standard / deep / code。
    /// 会话创建时钉住;已有会话可用 sessions.preset_id 覆盖。
    #[serde(default = "default_preset")]
    pub default_preset: String,
    /// 拖入文件夹时单文件夹最多加入的附件数(1..=50;默认 10)
    #[serde(default = "default_attachment_folder_limit")]
    pub attachment_folder_limit: u32,
    /// 附件扩展名黑名单的追加项(小写、不含点)。
    /// 内置默认清单(`attachment_guard::DEFAULT_BLOCKED_EXT`)始终生效,此处只能追加。
    #[serde(default)]
    pub attachment_blocked_ext: Vec<String>,
    /// 单个会话的附件数量配额(去重后的文件数;1..=2000;默认 200)
    #[serde(default = "default_attachment_session_max_files")]
    pub attachment_session_max_files: u32,
    /// 单个会话的附件容量配额(MB;1..=10240;默认 1024)
    #[serde(default = "default_attachment_session_max_mb")]
    pub attachment_session_max_mb: u32,
    /// 单次生成的总时长上限(分钟):覆盖连接、思考、流式输出与工具轮次。
    /// 默认 10 分钟;大工程可调大(上限 24 小时),到点后收口并保留已生成内容。
    #[serde(default = "default_chat_timeout_minutes")]
    pub chat_timeout_minutes: u32,
}

/// 生成时长下限(分钟):低于 1 分钟无法完成一次正常请求。
pub const MIN_CHAT_TIMEOUT_MINUTES: u32 = 1;
/// 生成时长上限(分钟):24 小时,避免误填极大值造成无限挂起。
pub const MAX_CHAT_TIMEOUT_MINUTES: u32 = 1440;
/// 生成时长默认值(分钟):与旧版固定 10 分钟一致,升级后行为不变。
pub const DEFAULT_CHAT_TIMEOUT_MINUTES: u32 = 10;

/// 单会话附件数量配额范围:1..=2000。
pub const MIN_ATTACHMENT_SESSION_MAX_FILES: u32 = 1;
pub const MAX_ATTACHMENT_SESSION_MAX_FILES: u32 = 2000;
/// 单会话附件容量配额范围(MB):1..=10240(10GB)。
pub const MIN_ATTACHMENT_SESSION_MAX_MB: u32 = 1;
pub const MAX_ATTACHMENT_SESSION_MAX_MB: u32 = 10240;
/// 配额默认值:200 个文件 / 1GB,足以覆盖常规长会话,又避免单会话无限膨胀。
pub const DEFAULT_ATTACHMENT_SESSION_MAX_FILES: u32 = 200;
pub const DEFAULT_ATTACHMENT_SESSION_MAX_MB: u32 = 1024;

fn default_attachment_session_max_files() -> u32 {
    DEFAULT_ATTACHMENT_SESSION_MAX_FILES
}

fn default_attachment_session_max_mb() -> u32 {
    DEFAULT_ATTACHMENT_SESSION_MAX_MB
}

/// 生效的单会话附件配额(读取设置并钳制到合法范围)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentQuota {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl AttachmentQuota {
    /// 容量配额的展示值(MB)
    pub fn max_mb(&self) -> u64 {
        self.max_bytes / (1024 * 1024)
    }
}

/// 读取当前单会话附件配额(每次保存附件时读取,设置改完即时生效)。
pub fn attachment_quota() -> AttachmentQuota {
    let s = load();
    let files = s
        .attachment_session_max_files
        .clamp(MIN_ATTACHMENT_SESSION_MAX_FILES, MAX_ATTACHMENT_SESSION_MAX_FILES);
    let mb = s
        .attachment_session_max_mb
        .clamp(MIN_ATTACHMENT_SESSION_MAX_MB, MAX_ATTACHMENT_SESSION_MAX_MB);
    AttachmentQuota {
        max_files: files as usize,
        max_bytes: mb as u64 * 1024 * 1024,
    }
}

fn default_chat_timeout_minutes() -> u32 {
    DEFAULT_CHAT_TIMEOUT_MINUTES
}

fn default_attachment_folder_limit() -> u32 {
    10
}

fn default_context_window() -> u32 {
    24
}

fn default_workbench() -> String {
    "chat".into()
}

fn default_preset() -> String {
    crate::presets::DEFAULT_PRESET.into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            summarize_provider_id: None,
            summarize_model: None,
            selection_ask_enabled: true,
            minimize_to_tray: false,
            hidden_nav_items: Vec::new(),
            nav_order: Vec::new(),
            context_window_messages: default_context_window(),
            bare_mode: false,
            first_run_completed: false,
            custom_base_prompt: None,
            custom_app_name: None,
            custom_app_icon: None,
            home_prompt: None,
            workspace_root: None,
            default_workbench: default_workbench(),
            default_preset: default_preset(),
            attachment_folder_limit: default_attachment_folder_limit(),
            attachment_blocked_ext: Vec::new(),
            attachment_session_max_files: default_attachment_session_max_files(),
            attachment_session_max_mb: default_attachment_session_max_mb(),
            chat_timeout_minutes: default_chat_timeout_minutes(),
        }
    }
}

fn settings_path() -> PathBuf {
    crate::paths::data_dir().join("settings.json")
}

/// 读取设置;文件缺失/损坏时返回默认值(不报错,保证本地优先可用)。
pub fn load() -> AppSettings {
    let p = settings_path();
    let Ok(text) = fs::read_to_string(&p) else {
        return AppSettings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 保存设置(覆盖写)。
pub fn save(settings: &AppSettings) -> Result<(), String> {
    let p = settings_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 生效的单次生成时长(分钟):读取设置并夹到 [1, 1440]。
/// 每个请求开始时读取一次,设置改完即刻对下一次请求生效。
pub fn chat_timeout_minutes() -> u32 {
    load()
        .chat_timeout_minutes
        .clamp(MIN_CHAT_TIMEOUT_MINUTES, MAX_CHAT_TIMEOUT_MINUTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_json_roundtrip() {
        let s = AppSettings {
            summarize_provider_id: Some("deepseek".into()),
            summarize_model: Some("deepseek-chat".into()),
            ..Default::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.summarize_provider_id.as_deref(), Some("deepseek"));
        assert_eq!(back.summarize_model.as_deref(), Some("deepseek-chat"));
        assert!(back.selection_ask_enabled);
        assert!(!back.minimize_to_tray);
        assert!(back.hidden_nav_items.is_empty());
    }

    #[test]
    fn settings_default_when_fields_missing() {
        // 旧版 settings.json(无字段)或空对象都能解析为默认
        let back: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(back.summarize_provider_id.is_none());
        assert!(back.summarize_model.is_none());
        assert!(back.selection_ask_enabled);
        assert!(!back.minimize_to_tray);
        // 生成时长未配置时沿用旧版行为(10 分钟)
        assert_eq!(back.chat_timeout_minutes, DEFAULT_CHAT_TIMEOUT_MINUTES);
        // 附件配额未配置时使用默认值(200 个 / 1GB)
        assert_eq!(
            back.attachment_session_max_files,
            DEFAULT_ATTACHMENT_SESSION_MAX_FILES
        );
        assert_eq!(
            back.attachment_session_max_mb,
            DEFAULT_ATTACHMENT_SESSION_MAX_MB
        );
    }

    #[test]
    fn attachment_quota_clamped_to_range() {
        let mut s = AppSettings::default();
        s.attachment_session_max_files = 0;
        s.attachment_session_max_mb = 0;
        let q = AttachmentQuota {
            max_files: s
                .attachment_session_max_files
                .clamp(MIN_ATTACHMENT_SESSION_MAX_FILES, MAX_ATTACHMENT_SESSION_MAX_FILES)
                as usize,
            max_bytes: s
                .attachment_session_max_mb
                .clamp(MIN_ATTACHMENT_SESSION_MAX_MB, MAX_ATTACHMENT_SESSION_MAX_MB)
                as u64
                * 1024
                * 1024,
        };
        assert_eq!(q.max_files, 1);
        assert_eq!(q.max_bytes, 1024 * 1024);

        s.attachment_session_max_files = u32::MAX;
        s.attachment_session_max_mb = u32::MAX;
        let text = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(
            back.attachment_session_max_files.min(MAX_ATTACHMENT_SESSION_MAX_FILES),
            MAX_ATTACHMENT_SESSION_MAX_FILES
        );
        assert_eq!(
            back.attachment_session_max_mb.min(MAX_ATTACHMENT_SESSION_MAX_MB),
            MAX_ATTACHMENT_SESSION_MAX_MB
        );
    }

    #[test]
    fn chat_timeout_minutes_clamped_to_range() {
        let mut s = AppSettings::default();
        s.chat_timeout_minutes = 0;
        assert_eq!(
            s.chat_timeout_minutes.clamp(MIN_CHAT_TIMEOUT_MINUTES, MAX_CHAT_TIMEOUT_MINUTES),
            MIN_CHAT_TIMEOUT_MINUTES
        );
        s.chat_timeout_minutes = 99999;
        assert_eq!(
            s.chat_timeout_minutes.clamp(MIN_CHAT_TIMEOUT_MINUTES, MAX_CHAT_TIMEOUT_MINUTES),
            MAX_CHAT_TIMEOUT_MINUTES
        );
        s.chat_timeout_minutes = 620;
        let text = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.chat_timeout_minutes, 620);
    }
}
