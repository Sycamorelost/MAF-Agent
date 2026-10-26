//! 系统状态只读快照(G5):概览 / 进程 / 磁盘 / 网络。
//!
//! 全部只读、免审批,每次调用落审计。快照含主机名 / 进程名等本机信息,
//! 仅返回给当前对话,不做任何写操作。CPU 占用率需要两次采样取差值,
//! 因此 overview/processes 内部有约 200ms 的采样间隔。

use serde_json::{Value, json};
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

/// 进程快照默认条数 / 上限。
const DEFAULT_PROC_LIMIT: usize = 30;
const MAX_PROC_LIMIT: usize = 100;

/// 概览:OS / 主机名 / CPU 核数与占用 / 内存 / swap / 运行时长。
pub fn overview() -> Value {
    let mut sys = System::new();
    sys.refresh_memory();
    // CPU 占用率需两次采样;间隔取 crate 建议的最小值
    sys.refresh_cpu_all();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_all();

    let cpus = sys.cpus();
    let usage = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| f64::from(c.cpu_usage())).sum::<f64>() / cpus.len() as f64
    };
    json!({
        "os": System::name().unwrap_or_default(),
        "os_version": System::os_version().unwrap_or_default(),
        "kernel": System::kernel_version().unwrap_or_default(),
        "host_name": System::host_name().unwrap_or_default(),
        "cpu_cores": System::physical_core_count().unwrap_or(cpus.len()),
        "cpu_logical": cpus.len(),
        "cpu_usage_percent": (usage * 10.0).round() / 10.0,
        "mem_total_mb": sys.total_memory() / 1_048_576,
        "mem_used_mb": sys.used_memory() / 1_048_576,
        "swap_total_mb": sys.total_swap() / 1_048_576,
        "swap_used_mb": sys.used_swap() / 1_048_576,
        "uptime_secs": System::uptime(),
    })
}

/// 进程快照:按内存占用降序,limit 收敛到 [1, 100]。
pub fn processes(limit: Option<usize>) -> Value {
    let limit = limit.unwrap_or(DEFAULT_PROC_LIMIT).clamp(1, MAX_PROC_LIMIT);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.refresh_cpu_all();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_processes(ProcessesToUpdate::All, false);
    sys.refresh_cpu_all();

    let mut list: Vec<Value> = sys
        .processes()
        .values()
        .map(|p| {
            json!({
                "pid": p.pid().as_u32(),
                "name": p.name().to_string_lossy(),
                "mem_mb": p.memory() / 1_048_576,
                "cpu_percent": (f64::from(p.cpu_usage()) * 10.0).round() / 10.0,
            })
        })
        .collect();
    list.sort_by(|a, b| {
        let ma = a["mem_mb"].as_u64().unwrap_or(0);
        let mb = b["mem_mb"].as_u64().unwrap_or(0);
        mb.cmp(&ma)
    });
    let total = list.len();
    list.truncate(limit);
    json!({ "total": total, "returned": list.len(), "processes": list })
}

/// 磁盘分区快照。
pub fn disks() -> Value {
    let disks = Disks::new_with_refreshed_list();
    let list: Vec<Value> = disks
        .iter()
        .map(|d| {
            json!({
                "name": d.name().to_string_lossy(),
                "mount": d.mount_point().to_string_lossy(),
                "fs": d.file_system().to_string_lossy(),
                "total_gb": d.total_space() / 1_073_741_824,
                "avail_gb": d.available_space() / 1_073_741_824,
                "removable": d.is_removable(),
            })
        })
        .collect();
    json!({ "count": list.len(), "disks": list })
}

/// 网络接口快照(名称 / IP / 累计收发)。
pub fn networks() -> Value {
    let nets = Networks::new_with_refreshed_list();
    let list: Vec<Value> = nets
        .iter()
        .map(|(name, d)| {
            json!({
                "name": name,
                "ips": d.ip_networks().iter().map(|n| n.to_string()).collect::<Vec<_>>(),
                "recv_mb": d.received() / 1_048_576,
                "sent_mb": d.transmitted() / 1_048_576,
            })
        })
        .collect();
    json!({ "count": list.len(), "networks": list })
}

/// 落审计(与 shell_exec 同一 JSONL)。
pub fn audit(tool: &str) {
    crate::mcp::audit(
        &crate::paths::data_dir(),
        "sys_snapshot",
        "builtin",
        tool,
        "ok",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_has_core_fields() {
        let v = overview();
        assert!(v["cpu_cores"].as_u64().unwrap_or(0) >= 1);
        assert!(v["mem_total_mb"].as_u64().unwrap_or(0) > 0);
        assert!(v["os"].as_str().is_some());
        assert!(v["uptime_secs"].as_u64().is_some());
    }

    #[test]
    fn processes_respects_limit_and_sort() {
        let v = processes(Some(5));
        let arr = v["processes"].as_array().unwrap();
        assert!(!arr.is_empty() && arr.len() <= 5);
        assert!(v["total"].as_u64().unwrap() >= arr.len() as u64);
        // 按内存降序
        let mems: Vec<u64> = arr.iter().map(|p| p["mem_mb"].as_u64().unwrap()).collect();
        let mut sorted = mems.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(mems, sorted);
    }

    #[test]
    fn processes_limit_clamped() {
        let v = processes(Some(999_999));
        assert!(v["returned"].as_u64().unwrap() <= MAX_PROC_LIMIT as u64);
    }

    #[test]
    fn disks_and_networks_return_arrays() {
        assert!(disks()["disks"].as_array().is_some());
        assert!(networks()["networks"].as_array().is_some());
        // Windows 下至少存在一个磁盘分区
        #[cfg(windows)]
        assert!(disks()["count"].as_u64().unwrap() >= 1);
    }
}
