//! 通用「Provider 流 → channel」后台任务(translate_stream 共用)。

use tauri::ipc::Channel;

use crate::provider::{ChatRequest, Provider, StreamChunk};

use super::super::stream::{StreamCancelState, run_cancellable};

/// 在后台任务中跑流并推到 channel,避免阻塞 IPC 主调用(translate_stream 共用)。
pub(crate) fn spawn_chat_to_channel(
    provider: Box<dyn Provider>,
    req: ChatRequest,
    channel: Channel<StreamChunk>,
    cancel_state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
) {
    let work = async move {
        match provider.chat_stream(req).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                while let Some(chunk) = stream.next().await {
                    // Usage chunk 仅用于用量归因,不转发前端
                    if matches!(chunk, StreamChunk::Usage { .. }) {
                        continue;
                    }
                    if channel.send(chunk).is_err() {
                        // 前端已关闭 channel,直接退出
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = channel.send(StreamChunk::Error {
                    message: e.to_string(),
                });
            }
        }
    };
    tauri::async_runtime::spawn(run_cancellable(cancel_state, cancel_id, cancel_rx, work));
}
