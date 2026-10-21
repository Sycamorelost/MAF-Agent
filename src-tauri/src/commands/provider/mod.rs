//! provider 相关 Tauri commands。

// 命令重导出用于保持 crate::commands::provider::xxx 原路径;注册表改走子模块路径后,
// 这些 re-export 在本 crate 内没有直接调用点。
#![allow(unused_imports)]

pub(crate) mod assembly;
pub(crate) mod chat;
pub(crate) mod failover;
pub(crate) mod image;
pub(crate) mod keys;
pub(crate) mod models;
pub(crate) mod pricing;
pub(crate) mod stream;
pub(crate) mod templates;
pub(crate) mod translate;
pub(crate) mod usage;

// ---------- 命令与对外类型(保持 crate::commands::provider::xxx 原路径可用)----------

pub use chat::{chat_stream, ChatStreamInput};
pub use image::{
    generate_image, image_to_session, list_images, read_image, GenerateImageInput, ImageListItem,
    ImageMeta, ImageResultItem,
};
pub use keys::{
    delete_api_key, delete_provider_key, get_rotation_configs, has_api_key, list_provider_keys,
    list_stored_provider_ids, save_api_key, save_provider_key, set_provider_rotation,
    update_key_slot, KeySlotView,
};
pub use models::{
    fetch_provider_models, get_provider_model_config, get_reasoning_capability,
    get_reasoning_override, list_model_cards, save_model_card, save_provider_model_config,
    save_reasoning_override,
};
pub use stream::{cancel_stream, StreamCancelState};
pub use templates::{
    delete_provider_template, list_provider_templates, restore_builtin_templates,
    save_provider_template,
};
pub use translate::{translate_stream, TranslateStreamInput};
pub use usage::{
    usage_entries, usage_hourly, usage_key_health, usage_recalc, usage_summary, UsageRecalcReport,
    UsageRecalcRow,
};

pub(crate) use assembly::{
    build_provider, build_provider_with_stream_options, validate_base_url_scheme,
};
pub(crate) use failover::is_stream_options_rejected;
pub(crate) use image::download_image_checked;
pub(crate) use keys::resolve_spec;

pub use crate::provider::model_cards::{load_model_cache, save_model_cache};
