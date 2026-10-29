//! 去 AI 味 · AI 生成概率分析(R3 B1):本地统计引擎。
//!
//! 设计要点(val.md §5):
//! - 三层引擎的第一层:纯 Rust 统计,零 token、毫秒级,默认开启;
//! - 只输出「写作特征」与证据,不给「人写 / AI 写」二值结论;
//! - 词表外置 `resources/aiflavor/patterns.json` 可热更,缺失时回退内嵌常量。

pub mod engine;
pub mod lexicon;
pub mod rules;

use std::path::PathBuf;
use std::sync::OnceLock;

use tauri::Manager;

static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 由 lib.rs setup 调用一次:解析打包资源目录(未命中时回退源码树)。
pub fn init_resource(app: &tauri::AppHandle) {
    if let Ok(dir) = app
        .path()
        .resolve("resources/aiflavor", tauri::path::BaseDirectory::Resource)
    {
        let _ = RESOURCE_DIR.set(dir);
    }
}

/// 资源目录:运行期解析优先;单测/未初始化时回退源码树。
pub fn dir() -> PathBuf {
    RESOURCE_DIR.get().cloned().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("aiflavor")
    })
}
