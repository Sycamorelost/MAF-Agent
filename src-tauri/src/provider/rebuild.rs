//! H3:stream_options 被中转站拒绝时,无该字段重建 Provider 所需的启动入参快照。
//! (P0 自 research/mod.rs 迁出;研究与学习规划两条流水线共用。)

/// 重建 Provider(关闭 stream_options)所需的启动入参快照。
pub struct ProviderRebuild {
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}
