//! 后台流式任务的启动与 panic 兜底。

use tauri::ipc::Channel;

use super::super::super::stream::{StreamCancelState, run_cancellable};
use crate::provider::StreamChunk;

/// 终态兜底:后台任务 panic 时 Tokio 只会吞掉 JoinHandle 的 panic,channel 静默关闭,
/// 前端 `done` 永不 resolve(表现为「思考/工具调用之后再无任何反应」)。
/// 这里 catch_unwind 后补发一个 Error chunk,保证前端一定收口。
pub(super) fn launch<F>(
    work: F,
    panic_channel: Channel<StreamChunk>,
    cancel_state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
) where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        use futures::FutureExt;
        let fut = run_cancellable(cancel_state, cancel_id, cancel_rx, work);
        if std::panic::AssertUnwindSafe(fut)
            .catch_unwind()
            .await
            .is_err()
        {
            log::error!("chat stream task panicked; emit Error chunk to close the stream");
            let _ = panic_channel.send(StreamChunk::Error {
                message: "对话流异常中断(内部错误),请重试".into(),
            });
        }
    });
}
