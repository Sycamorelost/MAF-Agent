//! 后台流式任务的启动与终态兜底。

use super::super::super::stream::{StreamCancelState, run_cancellable};
use crate::provider::StreamChunk;

use super::super::run::{RunSink, RunStatus};

/// 终态兜底:后台任务 panic 时 Tokio 只会吞掉 JoinHandle 的 panic,
/// 前端既收不到 chunk 也收不到结束事件(表现为「思考/工具调用之后再无任何反应」)。
/// 这里 catch_unwind 后补发 Error chunk 收口;用户取消则只广播 `chat:run_end(cancelled)`。
pub(super) fn launch<F>(
    work: F,
    sink: RunSink,
    cancel_state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
) where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        use futures::FutureExt;
        let fut = run_cancellable(cancel_state, cancel_id, cancel_rx, work);
        match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
            // 用户主动停止:不伪造 Error chunk,只把 run 标为 cancelled
            Ok(None) => sink.finish(RunStatus::Cancelled),
            // 正常返回:绝大多数路径已自行发送 Done/Error;遗漏时按 Done 兜底
            Ok(Some(())) => sink.finish(RunStatus::Done),
            Err(_) => {
                log::error!("chat run panicked; emit Error chunk to close the stream");
                let _ = sink.send(StreamChunk::Error {
                    message: "对话流异常中断(内部错误),请重试".into(),
                });
            }
        }
    });
}
