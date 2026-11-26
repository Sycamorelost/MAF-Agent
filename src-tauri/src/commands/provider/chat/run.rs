//! 后台对话 run:进程级注册表、带序号的事件广播与状态查询。
//!
//! 生成本身跑在 Tauri 后台任务里,不再持有前端 `Channel`:
//! 每个 chunk 先写入环形缓冲得到单调 `seq`,再以窗口事件 `chat:run` 广播;
//! 结束(正常/报错/取消)时额外广播 `chat:run_end`。
//! 前端因此可以随时挂载/卸载页面,凭 `chat_run_state(run_id, after_seq)` 重放。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::provider::StreamChunk;

/// 单个 chunk 事件名:payload `{ run_id, seq, chunk }`。
pub const EVENT_CHUNK: &str = "chat:run";
/// run 终态事件名:payload `{ run_id, status }`。
pub const EVENT_END: &str = "chat:run_end";

/// 单 run 缓冲上限;超出后丢弃最旧 chunk(与 `dropped_through` 一起标记截断)。
const MAX_BUFFERED: usize = 4096;
/// 终态 run 的保留时长(供前端刷新后重放)。
const KEEP_FINISHED: Duration = Duration::from_secs(15 * 60);

/// run 的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Done,
    Error,
    Cancelled,
}

/// 带序号的流式 chunk(seq 从 1 开始单调递增)。
#[derive(Debug, Clone, Serialize)]
pub struct SeqChunk {
    pub seq: u64,
    pub chunk: StreamChunk,
}

/// 一个后台生成任务的全部可观察状态。
pub struct RunEntry {
    pub id: String,
    pub session_id: Option<String>,
    seq: AtomicU64,
    chunks: Mutex<VecDeque<SeqChunk>>,
    /// 已被丢弃的最旧序号;`after_seq < dropped_through` 表示重放不完整。
    dropped_through: AtomicU64,
    status: Mutex<RunStatus>,
    finished_at: Mutex<Option<Instant>>,
}

// Mutex 中毒一律取回内部值继续用:注册表/缓冲属于观测设施,不能因一次 panic 让后续请求全崩。
fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl RunEntry {
    fn new(id: String, session_id: Option<String>) -> Self {
        Self {
            id,
            session_id,
            seq: AtomicU64::new(0),
            chunks: Mutex::new(VecDeque::new()),
            dropped_through: AtomicU64::new(0),
            status: Mutex::new(RunStatus::Running),
            finished_at: Mutex::new(None),
        }
    }

    /// 入缓冲并返回本 chunk 的序号。
    fn push(&self, chunk: StreamChunk) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let mut buf = lock_or_recover(&self.chunks);
        buf.push_back(SeqChunk { seq, chunk });
        while buf.len() > MAX_BUFFERED {
            if let Some(old) = buf.pop_front() {
                self.dropped_through.store(old.seq, Ordering::SeqCst);
            }
        }
        seq
    }

    fn status(&self) -> RunStatus {
        *lock_or_recover(&self.status)
    }

    fn is_running(&self) -> bool {
        self.status() == RunStatus::Running
    }

    /// 仅在 `Running` 时迁移到终态(幂等,保证 `chat:run_end` 只发一次)。
    fn finish_once(&self, status: RunStatus) -> bool {
        {
            let mut guard = lock_or_recover(&self.status);
            if *guard != RunStatus::Running {
                return false;
            }
            *guard = status;
        }
        *lock_or_recover(&self.finished_at) = Some(Instant::now());
        true
    }

    fn expired(&self, keep: Duration) -> bool {
        match *lock_or_recover(&self.finished_at) {
            Some(at) => at.elapsed() >= keep,
            None => false,
        }
    }

    /// 返回 `after_seq` 之后的 chunk,以及是否因环形缓冲丢弃而截断。
    fn snapshot(&self, after_seq: u64) -> (Vec<SeqChunk>, bool) {
        let buf = lock_or_recover(&self.chunks);
        let dropped = self.dropped_through.load(Ordering::SeqCst);
        (
            buf.iter().filter(|c| c.seq > after_seq).cloned().collect(),
            after_seq < dropped,
        )
    }
}

#[derive(Clone, Serialize)]
struct RunEvent {
    run_id: String,
    seq: u64,
    chunk: StreamChunk,
}

#[derive(Clone, Serialize)]
struct RunEndEvent {
    run_id: String,
    status: RunStatus,
}

/// 流式 chunk 发送端:写缓冲 + 广播事件。可跨任务克隆。
#[derive(Clone)]
pub struct RunSink {
    app: AppHandle,
    entry: Arc<RunEntry>,
}

impl RunSink {
    /// 缓冲并广播一个 chunk;`Done` / `Error` 会顺带收口 run。
    /// 恒返回 `Ok`(保留调用方既有的 `is_err()` 判断形态)。
    pub fn send(&self, chunk: StreamChunk) -> Result<(), String> {
        let terminal = match &chunk {
            StreamChunk::Done { .. } => Some(RunStatus::Done),
            StreamChunk::Error { .. } => Some(RunStatus::Error),
            _ => None,
        };
        let seq = self.entry.push(chunk.clone());
        let _ = self.app.emit(
            EVENT_CHUNK,
            RunEvent {
                run_id: self.entry.id.clone(),
                seq,
                chunk,
            },
        );
        if let Some(status) = terminal {
            self.finish(status);
        }
        Ok(())
    }

    /// 迁移到终态并广播 `chat:run_end`(重复调用无副作用)。
    pub fn finish(&self, status: RunStatus) {
        if self.entry.finish_once(status) {
            let _ = self.app.emit(
                EVENT_END,
                RunEndEvent {
                    run_id: self.entry.id.clone(),
                    status,
                },
            );
        }
    }
}

/// 进程级 run 注册表(Tauri State)。
#[derive(Default)]
pub struct ChatRunRegistry {
    runs: Mutex<HashMap<String, Arc<RunEntry>>>,
}

impl ChatRunRegistry {
    fn prune(map: &mut HashMap<String, Arc<RunEntry>>) {
        map.retain(|_, e| !e.expired(KEEP_FINISHED));
    }

    /// 登记一个新 run:run_id 必须唯一,且同一会话不允许并行两个生成。
    pub fn start(
        &self,
        app: AppHandle,
        run_id: String,
        session_id: Option<String>,
    ) -> Result<RunSink, String> {
        let mut map = lock_or_recover(&self.runs);
        Self::prune(&mut map);
        if map.contains_key(&run_id) {
            return Err("run_id 重复,请重试".into());
        }
        if let Some(sid) = session_id.as_deref()
            && map
                .values()
                .any(|e| e.session_id.as_deref() == Some(sid) && e.is_running())
        {
            return Err("该会话已有进行中的生成,请先停止当前生成".into());
        }
        let entry = Arc::new(RunEntry::new(run_id.clone(), session_id));
        map.insert(run_id, entry.clone());
        Ok(RunSink { app, entry })
    }

    fn get(&self, run_id: &str) -> Option<Arc<RunEntry>> {
        let mut map = lock_or_recover(&self.runs);
        Self::prune(&mut map);
        map.get(run_id).cloned()
    }
}

/// `chat_run_state` 的返回体。
#[derive(Debug, Clone, Serialize)]
pub struct RunStatePayload {
    pub run_id: String,
    pub status: RunStatus,
    pub chunks: Vec<SeqChunk>,
    pub truncated: bool,
}

/// 查询 run 状态与 `after_seq` 之后的 chunk(前端刷新/重挂时按序号重放)。
#[tauri::command]
pub fn chat_run_state(
    state: tauri::State<'_, ChatRunRegistry>,
    run_id: String,
    after_seq: Option<u64>,
) -> Result<RunStatePayload, String> {
    let entry = state
        .get(&run_id)
        .ok_or_else(|| "run 不存在或已过期".to_string())?;
    let (chunks, truncated) = entry.snapshot(after_seq.unwrap_or(0));
    Ok(RunStatePayload {
        run_id,
        status: entry.status(),
        chunks,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(text: &str) -> StreamChunk {
        StreamChunk::Delta {
            content: text.into(),
        }
    }

    fn push_n(entry: &RunEntry, n: usize) {
        for i in 0..n {
            entry.push(delta(&format!("d{i}")));
        }
    }

    #[test]
    fn seq_starts_at_one_and_grows() {
        let e = RunEntry::new("r".into(), None);
        assert_eq!(e.push(delta("a")), 1);
        assert_eq!(e.push(delta("b")), 2);
        assert_eq!(e.push(delta("c")), 3);
    }

    #[test]
    fn snapshot_returns_chunks_after_seq() {
        let e = RunEntry::new("r".into(), None);
        push_n(&e, 5);
        let (chunks, truncated) = e.snapshot(2);
        assert!(!truncated);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].seq, 3);
        assert!(e.snapshot(5).0.is_empty());
        assert_eq!(e.snapshot(0).0.len(), 5);
    }

    #[test]
    fn ring_buffer_drops_oldest_and_flags_truncation() {
        let e = RunEntry::new("r".into(), None);
        push_n(&e, MAX_BUFFERED + 3);
        let (chunks, truncated) = e.snapshot(0);
        assert!(truncated, "丢弃过旧 chunk 必须标记 truncated");
        assert_eq!(chunks.len(), MAX_BUFFERED);
        assert_eq!(chunks[0].seq, 4);
        // after_seq 已越过丢弃水位则不算截断
        assert!(!e.snapshot(MAX_BUFFERED as u64).1);
    }

    #[test]
    fn finish_is_idempotent_and_first_terminal_wins() {
        let e = RunEntry::new("r".into(), None);
        assert!(e.is_running());
        assert!(e.finish_once(RunStatus::Cancelled));
        assert!(!e.finish_once(RunStatus::Done));
        assert_eq!(e.status(), RunStatus::Cancelled);
        assert!(!e.is_running());
        assert!(
            lock_or_recover(&e.finished_at).is_some(),
            "终态必须记录完成时间供惰性清理"
        );
    }

    #[test]
    fn running_entry_never_expires() {
        let e = RunEntry::new("r".into(), None);
        assert!(!e.expired(Duration::ZERO));
        e.finish_once(RunStatus::Done);
        assert!(e.expired(Duration::ZERO));
    }
}
