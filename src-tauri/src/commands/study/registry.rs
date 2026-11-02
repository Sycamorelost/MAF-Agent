//! 运行中学习规划的控制标记与任务句柄注册表。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

/// 一次运行中规划的控制标记(Arc 共享给引擎任务)。
#[derive(Clone, Default)]
pub struct StudyCtl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// 注册表条目:控制标记 + 任务句柄。
pub struct StudyEntry {
    pub ctl: Arc<StudyCtl>,
    pub(crate) handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// 正在运行的规划任务控制表(id → 控制 + 句柄)。
#[derive(Clone, Default)]
pub struct StudyRegistry(pub Arc<Mutex<HashMap<String, Arc<StudyEntry>>>>);
