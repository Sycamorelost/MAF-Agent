//! 流式请求取消(M-2):取消信号与进程级注册表。

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
#[derive(Clone, Default)]
pub struct StreamCancelState {
    map: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<StreamCancel>>>,
    >,
}

impl StreamCancelState {
    /// 注册并返回(信号句柄, 取消接收端)。接收端交给 run_cancellable 等待。
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
        match self.map.lock() {
            Ok(mut m) => {
                // N-3:同 id 重复注册时先取消旧任务,避免旧任务永远无法被取消
                if let Some(old) = m.insert(id.to_string(), token.clone()) {
                    old.fire();
                }
            }
            // N-3:Mutex 中毒不 panic( unwrap 会让后续所有流式命令崩溃);
            // 退化为本次不可取消,流本身仍可用
            Err(e) => log::warn!("stream cancel registry poisoned: {e}"),
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
#[tauri::command]
pub fn cancel_stream(state: tauri::State<'_, StreamCancelState>, cancel_id: String) {
    let token = state
        .map
        .lock()
        .ok()
        .and_then(|m| m.get(&cancel_id).cloned());
    if let Some(t) = token {
        t.fire();
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
