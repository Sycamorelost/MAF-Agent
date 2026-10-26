//! memory.md 文件 watcher。
//!
//! 对应 plan/01-phase1-mvp.md §3.2 watch_memory:
//!   外部编辑器改 memory.md → 触发 `memory_changed` 事件 → 前端面板自动刷新。
//!
//! 用 notify crate;每个会话一个独立 watcher,挂在 tauri State 里管理生命周期。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Mutex,
        mpsc::{self, RecvTimeoutError, SyncSender, TrySendError},
    },
    time::Duration,
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use super::memory_path;

/// memory_changed 事件的 trailing 去抖窗口(M-11):
/// 编辑器一次保存会产生多个 modify/create 事件,等待静默 200ms 后只广播一次,
/// 同时避免在文件尚未写完整时触发前端 reload / 标题回读。
const DEBOUNCE_MS: u64 = 200;

/// 反向同步:外部编辑器(VSCode 等)改了 memory.md frontmatter 的 conversation 字段时,
/// 把新标题写回 sessions.title,并广播 `session_renamed` 事件让前端侧栏/会话头刷新。
/// 仅在标题确实变化时写库,避免 watcher 自触发循环(sync_title 写文件也会触发本回调)。
fn sync_title_from_memory(session_id: &str, app: &AppHandle) {
    let Ok(text) = std::fs::read_to_string(memory_path(session_id)) else {
        return;
    };
    let Ok((fm, _)) = super::frontmatter::parse(&text) else {
        return;
    };
    let new_title = fm.conversation.trim().to_string();
    if new_title.is_empty() {
        return;
    }
    let Ok(Some(current)) = crate::db::sessions::get(session_id) else {
        return;
    };
    if current.title == new_title {
        return;
    }
    if crate::db::sessions::update_title(session_id, &new_title).is_ok() {
        let _ = app.emit(
            "session_renamed",
            serde_json::json!({
                "session_id": session_id,
                "title": new_title,
            }),
        );
    }
}

/// 全局 watcher 表:session_id -> watcher 句柄。
/// 句柄只是保活;事件通过 channel 线程转发。
pub struct WatcherRegistry {
    map: Mutex<HashMap<String, RecommendedWatcher>>,
}

impl Default for WatcherRegistry {
    fn default() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl WatcherRegistry {
    /// 开始监听某会话的 memory.md。重复调用幂等(已存在则跳过)。
    pub fn watch(&self, app: AppHandle, session_id: &str) -> Result<(), String> {
        let mut map = self.map.lock().map_err(|e| e.to_string())?;
        if map.contains_key(session_id) {
            return Ok(());
        }

        let path: PathBuf = memory_path(session_id);
        if !path.exists() {
            return Err(format!("memory.md not exists: {}", path.display()));
        }

        let sid = session_id.to_string();
        let app_clone = app.clone();
        // 有界容量 1 的通知通道:watch 回调只负责"敲门";去抖线程合并静默窗口。
        // 容量设 1:窗口内后续事件 try_send 满了直接丢弃,正合去抖语义。
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let tx_for_watcher: SyncSender<()> = tx.clone();
        let watcher_sid = sid.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // 只关心写操作,过滤临时文件噪音
                let dominated_by_write = matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                );
                if dominated_by_write {
                    // 非阻塞敲门:通道满说明去抖线程的静默计时已在进行,丢弃即可
                    if let Err(TrySendError::Disconnected(())) = tx_for_watcher.try_send(()) {
                        log::warn!("memory watcher 去抖线程已退出(session {watcher_sid})");
                    }
                }
            }
        })
        .map_err(|e| format!("create watcher failed: {e}"))?;

        // 去抖线程:首个事件到达后,持续吞掉静默期内的后续事件,
        // 直到 DEBOUNCE_MS 内无新事件才广播一次。tx 随 watcher drop 时线程自动退出。
        let debounce_app = app_clone.clone();
        let debounce_sid = sid.clone();
        std::thread::Builder::new()
            .name(format!("memory-debounce-{session_id}"))
            .spawn(move || {
                while rx.recv().is_ok() {
                    loop {
                        match rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS)) {
                            // 静默期内又来事件:重新计时
                            Ok(()) => continue,
                            // 静默窗口结束:广播一次最终状态
                            Err(RecvTimeoutError::Timeout) => {
                                let _ = debounce_app.emit(
                                    "memory_changed",
                                    serde_json::json!({
                                        "session_id": debounce_sid,
                                        "debounced": true,
                                    }),
                                );
                                // 反向同步:外部编辑器改了 frontmatter 的 conversation → 更新 sessions.title
                                sync_title_from_memory(&debounce_sid, &debounce_app);
                                break;
                            }
                            // watcher 已销毁(发送端全部 drop):线程收尾
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            })
            .map_err(|e| format!("spawn debounce thread failed: {e}"))?;
        // 局部 tx 在此 drop;仅 watcher 闭包持有一个发送端,其 drop 即通知线程退出
        drop(tx);

        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("watch {} failed: {e}", path.display()))?;

        map.insert(session_id.to_string(), watcher);
        log::info!("watching memory.md for session {}", session_id);
        Ok(())
    }

    /// 停止监听某会话。
    pub fn unwatch(&self, session_id: &str) -> Result<(), String> {
        let mut map = self.map.lock().map_err(|e| e.to_string())?;
        if let Some(mut w) = map.remove(session_id) {
            let _ = w.unwatch(&memory_path(session_id));
        }
        Ok(())
    }
}

/// 给 tauri State 用的包装,便于 Builder.manage()。
#[derive(Default)]
pub struct WatcherState {
    pub registry: WatcherRegistry,
}
