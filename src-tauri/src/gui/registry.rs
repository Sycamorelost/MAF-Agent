//! UI 元素短句柄注册表。
//!
//! 只保存「描述信息」而不保存 COM 接口指针:动作执行时按 automation_id / 名称 + 矩形
//! 重新定位元素,从而不必关心 UI Automation 跨线程套间(marshaling)问题。

use std::sync::{Arc, LazyLock, Mutex};

use uuid::Uuid;

use super::ElementInfo;

/// 注册表最多保留的条目数(超出时丢弃最旧的)。
const MAX_ENTRIES: usize = 200;

struct Entry {
    created_at: i64,
    info: ElementInfo,
}

#[derive(Clone, Default)]
pub struct ElementRegistry {
    inner: Arc<Mutex<Vec<Entry>>>,
}

/// 进程级单例 —— dispatcher 里的内置工具没有 State 注入,用全局注册表最直接。
pub static REGISTRY: LazyLock<ElementRegistry> = LazyLock::new(ElementRegistry::default);

impl ElementRegistry {
    /// 登记一个元素并分配短 id(8 位十六进制,便于模型引用)。
    pub fn add(&self, mut info: ElementInfo) -> ElementInfo {
        let id = Uuid::new_v4().to_string();
        info.element_id = id[..8].to_string();
        let mut guard = self.lock();
        guard.push(Entry {
            created_at: now_secs(),
            info: info.clone(),
        });
        if guard.len() > MAX_ENTRIES {
            // 去掉最旧的(元素句柄本身有 TTL,旧条目优先淘汰)
            guard.sort_by_key(|e| e.created_at);
            let overflow = guard.len() - MAX_ENTRIES;
            guard.drain(0..overflow);
        }
        info
    }

    /// 取出仍有效的元素描述。
    pub fn get(&self, id: &str, ttl_secs: i64) -> Option<ElementInfo> {
        let now = now_secs();
        let mut guard = self.lock();
        guard.retain(|e| now - e.created_at <= ttl_secs);
        guard
            .iter()
            .find(|e| e.info.element_id == id)
            .map(|e| e.info.clone())
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ElementInfo {
        ElementInfo {
            element_id: String::new(),
            hwnd: 1,
            process: "notepad.exe".into(),
            name: "保存".into(),
            automation_id: "SaveButton".into(),
            control_type: "button".into(),
            class_name: "Button".into(),
            enabled: true,
            rect: [0, 0, 10, 10],
        }
    }

    #[test]
    fn add_get_and_clear() {
        let reg = ElementRegistry::default();
        let info = reg.add(sample());
        assert_eq!(info.element_id.len(), 8);
        assert!(reg.get(&info.element_id, 60).is_some());
        assert!(reg.get("missing", 60).is_none());
        reg.clear();
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn expired_entries_are_dropped() {
        let reg = ElementRegistry::default();
        let info = reg.add(sample());
        assert!(
            reg.get(&info.element_id, -1).is_none(),
            "TTL 为负应立即过期"
        );
    }
}
