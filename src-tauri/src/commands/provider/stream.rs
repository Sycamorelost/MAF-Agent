//! 流式请求取消(M-2):取消信号与进程级注册表。

use std::collections::HashSet;

// ---------- 流式请求取消(M-2)----------

/// 单个流式请求的取消信号:oneshot 单发,run_cancellable 的 select 另一端在等。
pub(crate) struct StreamCancel {
    tx: std::sync::Mutex<Option<futures::channel::oneshot::Sender<()>>>,
}

impl StreamCancel {
    fn fire(&self) {
        if let Ok(mut g) = self.tx.lock()
            && let Some(tx) = g.take()
        {
            let _ = tx.send(());
        }
    }
}

/// 进程级流式取消注册表:cancel_id(前端生成) → 取消信号,任务结束自行注销。
/// P0-2d:新增 pending 集合 —— cancel 早于 register 到达时先存起来,register 成功后自动 fire。
#[derive(Clone, Default)]
pub struct StreamCancelState {
    map: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<StreamCancel>>>,
    >,
    /// P0-2d:cancel 早于 register 到达的 cancel_id
    pending: std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
}

impl StreamCancelState {
    /// 注册并返回(信号句柄, 取消接收端)。接收端交给 run_cancellable 等待。
    /// P0-2d:register 成功后立即检查 pending,如果 cancel 已先到达则 auto-fire。
    pub(crate) fn register(
        &self,
        id: &str,
    ) -> (
        std::sync::Arc<StreamCancel>,
        futures::channel::oneshot::Receiver<()>,
    ) {
        let (tx, rx) = futures::channel::oneshot::channel();
        let token = std::sync::Arc::new(StreamCancel {
            tx: std::sync::Mutex::new(Some(tx)),
        });
        let should_fire = {
            let mut fired_from_pending = false;
            match self.map.lock() {
                Ok(mut m) => {
                    // N-3:同 id 重复注册时先取消旧任务,避免旧任务永远无法被取消
                    if let Some(old) = m.insert(id.to_string(), token.clone()) {
                        old.fire();
                    }
                    // P0-2d:register 后立即检查 pending —— cancel 早到就 auto-fire
                    if let Ok(mut pending) = self.pending.lock() {
                        if pending.remove(id) {
                            fired_from_pending = true;
                        }
                    }
                }
                // N-3:Mutex 中毒不 panic( unwrap 会让后续所有流式命令崩溃);
                // 退化为本次不可取消,流本身仍可用
                Err(e) => log::warn!("stream cancel registry poisoned: {e}"),
            }
            fired_from_pending
        };
        if should_fire {
            token.fire();
        }
        (token, rx)
    }

    pub(crate) fn remove(&self, id: &str) {
        if let Ok(mut m) = self.map.lock() {
            m.remove(id);
        }
    }
}

/// 前端「停止生成/停止翻译」入口:触发取消信号。被 select 掉的工作 future
/// drop 时,内部 reqwest 响应流随之 drop、HTTP 连接中止,后端立即停止烧 token。
/// P0-2d:如果 cancel_id 还没注册(前端 cancel 早于后端 register),先存 pending,
/// register 成功时会自动补发取消信号。
#[tauri::command]
pub fn cancel_stream(state: tauri::State<'_, StreamCancelState>, cancel_id: String) {
    let fired = {
        let guard = state.map.lock();
        guard.ok().and_then(|m| m.get(&cancel_id).cloned()).and_then(|t| {
            t.fire();
            Some(())
        }).is_some()
    };
    if !fired {
        // P0-2d:还没注册,先存 pending,register 成功时 auto-fire
        if let Ok(mut pending) = state.pending.lock() {
            pending.insert(cancel_id);
        }
    }
}

/// 跑到完成或收到取消;无论结果如何都注销 cancel_id。返回 None 表示被取消。
/// 注意:work 整个 future 被 drop 即级联中止其内部的 HTTP 请求(连接关闭)。
pub(crate) async fn run_cancellable<T>(
    state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
    work: impl std::future::Future<Output = T>,
) -> Option<T> {
    use futures::FutureExt;
    let work = work.fuse();
    let cancel = cancel_rx.map(|_| ()).fuse();
    futures::pin_mut!(work);
    futures::pin_mut!(cancel);
    let result = futures::select! {
        v = work => Some(v),
        _ = cancel => None,
    };
    state.remove(&cancel_id);
    result
}
