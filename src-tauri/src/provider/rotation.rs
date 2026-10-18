//! API 流转(多 Key 轮换)的进程级状态与选槽策略。
//!
//! 对应 DEVPLAN-api-rotation-usage.md §4.2。
//!
//! 归因主线(两条功能共享):
//!   选槽 → 请求携带 slot → 响应 usage → usage_record 落库(slot/label/掩码)
//!
//! 设计要点:
//!   - 同一 Provider 的**多把 Key 轮流承担请求**;每次仅使用其中一把。
//!   - 轮换状态为**进程级共享**(Tauri Managed State),保证并发会话不撞同一把 Key。
//!   - 选槽是**纯函数**,冷却表与游标由调用方(状态)持有,便于单测。
//!   - Phase 3 编组:同一 base_url 的多个 Provider 可并入同一候选池,共享游标与冷却表;
//!     候选池以 (provider_id, slot) 定位,故冷却表键为 "{provider_id}#{slot}"。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

fn default_strategy() -> String {
    "round_robin".into()
}

fn default_cooldown() -> u64 {
    60
}

fn default_true() -> bool {
    true
}

fn default_weight() -> u32 {
    1
}

/// 单个 Key 槽位的用户元数据(标签 / 启用开关 / 权重 / 优先级)。
/// Key 本身永远加密存放在 keychain,绝不进入配置文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySlotMeta {
    pub slot: u32,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 权重:相对流量份额(>=1,默认 1)。权重 2 的槽位承担约两倍请求量。
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// 优先级:数值越大越先被使用;仅当高优先级槽位全部冷却时才降级到低优先级。
    #[serde(default)]
    pub priority: u32,
}

/// 候选 Key:(所属 provider, 槽位) + 权重 + 优先级。
/// 与配置解耦(便于纯函数单测),并支持跨 Provider 编组后的混合候选池。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCandidate {
    pub provider_id: String,
    pub slot: u32,
    pub weight: u32,
    pub priority: u32,
}

impl KeyCandidate {
    /// 等权、同级候选(默认 weight=1 / priority=0)。
    pub fn new(provider_id: impl Into<String>, slot: u32) -> Self {
        Self {
            provider_id: provider_id.into(),
            slot,
            weight: 1,
            priority: 0,
        }
    }

    /// 冷却表键:provider 内槽位唯一。
    pub fn cooldown_key(&self) -> String {
        cooldown_key(&self.provider_id, self.slot)
    }
}

/// 冷却表键(供 UI 查询某槽位剩余冷却)。
pub fn cooldown_key(provider_id: &str, slot: u32) -> String {
    format!("{provider_id}#{slot}")
}

/// 每 provider 的轮换配置(data/rotation.json)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// 是否启用多 Key 轮换;false 时退化为只用 slot 0(与历史行为完全一致)。
    #[serde(default)]
    pub enabled: bool,
    /// 轮换策略;目前仅 round_robin。
    #[serde(default = "default_strategy")]
    pub strategy: String,
    /// 触发限流后的冷却秒数(未解析到 Retry-After 时的默认值)。
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
    /// 该中转站是否支持 `stream_options`;false 时不发送该字段(兼容回退开关)。
    #[serde(default = "default_true")]
    pub supports_stream_options: bool,
    /// 槽位元数据(标签 / 启用 / 展示顺序)。
    #[serde(default)]
    pub keys: Vec<KeySlotMeta>,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: default_strategy(),
            cooldown_secs: default_cooldown(),
            supports_stream_options: true,
            keys: Vec::new(),
        }
    }
}

/// 轮换配置文件:`<data_dir>/rotation.json`,结构 {provider_id: RotationConfig}。
fn config_path() -> PathBuf {
    crate::paths::data_dir().join("rotation.json")
}

/// 读取全部 provider 的轮换配置(文件缺失/损坏时返回空表)。
pub fn load_configs() -> HashMap<String, RotationConfig> {
    let p = config_path();
    if !p.exists() {
        return HashMap::new();
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 整体落盘轮换配置。
pub fn save_configs(cfg: &HashMap<String, RotationConfig>) -> Result<(), String> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write rotation.json failed: {e}"))
}

/// 取某 provider 的配置;不存在时返回默认(未启用轮换)。
pub fn config_for(provider_id: &str) -> RotationConfig {
    load_configs().remove(provider_id).unwrap_or_default()
}

// ---------------- 运行时状态 ----------------

/// 每组(命名空间)的运行时轮换状态。
#[derive(Default)]
struct GroupRotation {
    /// 下次选槽的起始下标(相对当前可用候选环)。
    cursor: u32,
    /// "{provider_id}#{slot}" → 冷却结束时刻。
    cooldown: HashMap<String, Instant>,
}

/// 进程级轮换状态。`Arc` 包裹以便在 spawn 的后台任务中克隆持有。
#[derive(Clone, Default)]
pub struct RotationState {
    inner: Arc<Mutex<HashMap<String, GroupRotation>>>,
}

/// 纯函数:从候选池中挑下一个。
///   - 仅在**当前最高优先级**的非冷却候选中选择(高优先级全冷却才降级到次级);
///   - 把可用候选按权重展开成环,从 cursor 起取下一个 —— 权重高的候选出现次数更多;
///   - cursor 是**当前可用环**上的位置指针(可用集合变化时指向的候选会随之变化);
///   - 全部候选都在冷却时降级:返回冷却最早结束的候选(避免直接失败,仍尝试一次);
///   - cands 为空返回 None。
///
/// 返回 (选中的候选下标, 下一次的 cursor)。
pub fn pick_index(
    cands: &[KeyCandidate],
    cooldown: &HashMap<String, Instant>,
    now: Instant,
    cursor: u32,
) -> Option<(usize, u32)> {
    if cands.is_empty() {
        return None;
    }
    let cooling = |c: &KeyCandidate| cooldown.get(&c.cooldown_key()).is_some_and(|until| *until > now);

    let mut ready: Vec<usize> = (0..cands.len()).filter(|i| !cooling(&cands[*i])).collect();
    if ready.is_empty() {
        // 全冷却:选剩余冷却最短的候选降级尝试(游标保持不变)
        let idx = (0..cands.len())
            .min_by_key(|i| cooldown.get(&cands[*i].cooldown_key()).copied().unwrap_or(now))?;
        return Some((idx, cursor));
    }
    let top = ready.iter().map(|i| cands[*i].priority).max().unwrap_or(0);
    ready.retain(|i| cands[*i].priority == top);

    // 权重展开:weight 2 → 该候选在环上出现两次
    let ring: Vec<usize> = ready
        .iter()
        .flat_map(|i| std::iter::repeat_n(*i, cands[*i].weight.clamp(1, 50) as usize))
        .collect();
    let start = (cursor as usize) % ring.len();
    Some((ring[start], ((start + 1) % ring.len()) as u32))
}

impl RotationState {
    /// 选下一个可用候选并推进游标;cands 为空返回 None。
    /// `group` 为轮换状态命名空间:同一 base_url 编组的多个 Provider 用同一个 group,
    /// 从而共享游标与冷却表(未编组时通常直接传 provider_id)。
    pub fn pick(&self, group: &str, cands: &[KeyCandidate]) -> Option<usize> {
        let now = Instant::now();
        let keys: Vec<String> = cands.iter().map(|c| c.cooldown_key()).collect();
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let st = map.entry(group.to_string()).or_default();
        // 清理已失效(过期或槽位已删)的冷却记录
        st.cooldown.retain(|k, until| keys.contains(k) && *until > now);
        let (idx, next) = pick_index(cands, &st.cooldown, now, st.cursor)?;
        st.cursor = next;
        Some(idx)
    }

    /// 标记某槽位进入冷却(until 由 Retry-After 或默认冷却算出)。
    pub fn mark_cooldown(&self, group: &str, key: String, until: Instant) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(group.to_string()).or_default().cooldown.insert(key, until);
    }

    /// 便捷:按秒数标记冷却。
    pub fn mark_cooldown_secs(&self, group: &str, provider_id: &str, slot: u32, secs: u64) {
        self.mark_cooldown(
            group,
            cooldown_key(provider_id, slot),
            Instant::now() + Duration::from_secs(secs.max(1)),
        );
    }

    /// 某槽位剩余冷却秒数(未冷却返回 0)。
    pub fn cooldown_remaining(&self, group: &str, provider_id: &str, slot: u32) -> u64 {
        let now = Instant::now();
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(group)
            .and_then(|st| st.cooldown.get(&cooldown_key(provider_id, slot)))
            .filter(|until| **until > now)
            .map(|until| until.duration_since(now).as_secs() + 1)
            .unwrap_or(0)
    }

    /// 清理某槽位的冷却记录(删除 Key 时调用,避免重新添加同槽位时继承旧冷却)。
    pub fn clear_cooldown(&self, group: &str, provider_id: &str, slot: u32) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = map.get_mut(group) {
            st.cooldown.remove(&cooldown_key(provider_id, slot));
        }
    }

    /// 清理某组(命名空间)的运行时状态。
    pub fn clear(&self, group: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cd(pairs: &[(&str, u32, u64)]) -> HashMap<String, Instant> {
        let now = Instant::now();
        pairs
            .iter()
            .map(|(p, s, secs)| (cooldown_key(p, *s), now + Duration::from_secs(*secs)))
            .collect()
    }

    /// 单 Provider 等权候选构造。
    fn eq_cands(slots: &[u32]) -> Vec<KeyCandidate> {
        slots.iter().map(|s| KeyCandidate::new("p", *s)).collect()
    }

    /// 顺序轮询:游标依次推进。
    #[test]
    fn pick_round_robin_advances() {
        let now = Instant::now();
        let cands = eq_cands(&[0u32, 1, 2]);
        let none = HashMap::new();
        let (i0, c0) = pick_index(&cands, &none, now, 0).unwrap();
        assert_eq!((cands[i0].slot, c0), (0, 1));
        let (i1, c1) = pick_index(&cands, &none, now, c0).unwrap();
        assert_eq!((cands[i1].slot, c1), (1, 2));
        let (i2, c2) = pick_index(&cands, &none, now, c1).unwrap();
        assert_eq!((cands[i2].slot, c2), (2, 0));
        // 回到起点
        let (i3, _) = pick_index(&cands, &none, now, c2).unwrap();
        assert_eq!(cands[i3].slot, 0);
    }

    /// 跳过冷却中的槽位:环内只保留当前可用候选。
    #[test]
    fn pick_skips_cooling_slots() {
        let now = Instant::now();
        let cands = eq_cands(&[0u32, 1, 2]);
        let cooldown = cd(&[("p", 0, 30)]);
        // 0 在冷却 → 可用环 = [1, 2],cursor 0 取 1、cursor 1 取 2
        let (i, c) = pick_index(&cands, &cooldown, now, 0).unwrap();
        assert_eq!((cands[i].slot, c), (1, 1));
        let (i, _) = pick_index(&cands, &cooldown, now, 1).unwrap();
        assert_eq!(cands[i].slot, 2);
        // 两个槽位都在冷却 → 降级选冷却最早结束的槽位(1 更早)
        let cd_all = cd(&[("p", 0, 100), ("p", 1, 50)]);
        let cands2 = eq_cands(&[0u32, 1]);
        let (i, _) = pick_index(&cands2, &cd_all, now, 1).unwrap();
        assert_eq!(cands2[i].slot, 1, "全冷却时选冷却最早结束的槽位");
    }

    /// 冷却到期后重新可用。
    #[test]
    fn pick_uses_slot_after_cooldown_expires() {
        let cands = eq_cands(&[0u32, 1]);
        let now = Instant::now();
        let cooldown = HashMap::from([(cooldown_key("p", 0), now - Duration::from_secs(1))]);
        let (i, _) = pick_index(&cands, &cooldown, now, 0).unwrap();
        assert_eq!(cands[i].slot, 0);
    }

    /// 全部冷却 → 降级选冷却最早结束的槽位,而非直接失败。
    #[test]
    fn pick_degrades_when_all_cooling() {
        let now = Instant::now();
        let cands = eq_cands(&[0u32, 1, 2]);
        let cooldown = cd(&[("p", 0, 100), ("p", 1, 5), ("p", 2, 50)]);
        let (i, _) = pick_index(&cands, &cooldown, now, 0).unwrap();
        assert_eq!(cands[i].slot, 1, "应选冷却最早结束的槽位");
    }

    /// 无可用候选返回 None。
    #[test]
    fn pick_empty_slots_is_none() {
        let now = Instant::now();
        assert!(pick_index(&[], &HashMap::new(), now, 0).is_none());
    }

    /// 权重:weight=2 的候选在环上出现两次,承担约两倍流量。
    #[test]
    fn pick_weighted_shares_ratio() {
        let now = Instant::now();
        let none = HashMap::new();
        let cands = vec![
            KeyCandidate { provider_id: "p".into(), slot: 0, weight: 2, priority: 0 },
            KeyCandidate { provider_id: "p".into(), slot: 1, weight: 1, priority: 0 },
        ];
        let mut cursor = 0;
        let mut seq = Vec::new();
        for _ in 0..6 {
            let (i, c) = pick_index(&cands, &none, now, cursor).unwrap();
            seq.push(cands[i].slot);
            cursor = c;
        }
        // 环 = [0,0,1]
        assert_eq!(seq, vec![0, 0, 1, 0, 0, 1]);
    }

    /// 优先级:高优先级候选可用时永不使用低优先级;高优先级全冷却才降级。
    #[test]
    fn pick_respects_priority() {
        let now = Instant::now();
        let none = HashMap::new();
        let cands = vec![
            KeyCandidate { provider_id: "p".into(), slot: 0, weight: 1, priority: 0 },
            KeyCandidate { provider_id: "p".into(), slot: 1, weight: 1, priority: 5 },
        ];
        let mut cursor = 0;
        for _ in 0..4 {
            let (i, c) = pick_index(&cands, &none, now, cursor).unwrap();
            assert_eq!(cands[i].slot, 1, "高优先级候选可用时必须优先使用");
            cursor = c;
        }
        // 高优先级冷却 → 降级到低优先级
        let cooldown = cd(&[("p", 1, 60)]);
        let (i, _) = pick_index(&cands, &cooldown, now, 0).unwrap();
        assert_eq!(cands[i].slot, 0);
    }

    /// 编组:同一池内不同 Provider 的 Key 交叉轮换。
    #[test]
    fn pick_groups_across_providers() {
        let now = Instant::now();
        let cands = vec![
            KeyCandidate::new("a", 0),
            KeyCandidate::new("b", 0),
            KeyCandidate::new("b", 1),
        ];
        let none = HashMap::new();
        let (i0, c0) = pick_index(&cands, &none, now, 0).unwrap();
        let (i1, c1) = pick_index(&cands, &none, now, c0).unwrap();
        let (i2, _) = pick_index(&cands, &none, now, c1).unwrap();
        assert_eq!(
            (
                cands[i0].provider_id.as_str(),
                cands[i0].slot,
                cands[i1].provider_id.as_str(),
                cands[i1].slot,
                cands[i2].provider_id.as_str(),
                cands[i2].slot
            ),
            ("a", 0, "b", 0, "b", 1)
        );
    }

    /// 状态:冷却标记与剩余时间查询。
    #[test]
    fn state_marks_and_reports_cooldown() {
        let st = RotationState::default();
        let cands = eq_cands(&[0u32, 1]);
        assert_eq!(st.pick("g", &cands).map(|i| cands[i].slot), Some(0));
        assert_eq!(st.pick("g", &cands).map(|i| cands[i].slot), Some(1));
        st.mark_cooldown_secs("g", "p", 0, 30);
        assert!(st.cooldown_remaining("g", "p", 0) > 0);
        assert_eq!(st.cooldown_remaining("g", "p", 1), 0);
        // 0 冷却中 → 只挑得到 1
        assert_eq!(st.pick("g", &cands).map(|i| cands[i].slot), Some(1));
        st.clear("g");
        assert_eq!(st.cooldown_remaining("g", "p", 0), 0);
    }

    /// 编组:不同 Provider 共享同一 group 的冷却表(冷却 A 不影响 B)。
    #[test]
    fn state_shares_group_and_isolates_keys() {
        let st = RotationState::default();
        let cands = vec![KeyCandidate::new("a", 0), KeyCandidate::new("b", 0)];
        st.mark_cooldown_secs("grp", "a", 0, 30);
        assert!(st.cooldown_remaining("grp", "a", 0) > 0);
        assert_eq!(st.cooldown_remaining("grp", "b", 0), 0);
        // 冷却中的 a 被跳过 → 只能选到 b
        let i = st.pick("grp", &cands).unwrap();
        assert_eq!(cands[i].provider_id, "b");
    }
}
