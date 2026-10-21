//! 深度研究相关 Tauri commands(Phase 2)。

use std::collections::HashMap;
use std::sync::{
    atomic::AtomicBool,
    Arc, Mutex,
};

mod events;
mod export;
mod figure;
mod list;
mod references;
mod start;

pub use events::*;
pub use export::*;
pub use figure::*;
pub use list::*;
pub use references::*;
pub use start::*;

/// 一次运行中研究的控制标记(Arc 共享给引擎任务)。
#[derive(Clone, Default)]
pub struct ResearchCtl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// 注册表条目:控制标记 + 引擎任务句柄(删除研究时凭句柄等待任务真正退出,M-4)。
pub struct ResearchEntry {
    pub ctl: Arc<ResearchCtl>,
    /// 任务启动后回填;删除时 take 走并 await。任务自身结束时会从注册表移除整个条目。
    pub(crate) handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// 正在运行的研究任务的控制表(id → 控制标记 + 句柄)。
#[derive(Clone, Default)]
pub struct ResearchRegistry(pub Arc<Mutex<HashMap<String, Arc<ResearchEntry>>>>);
