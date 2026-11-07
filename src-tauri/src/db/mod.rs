//! db 模块入口。
//!
//! 数据落盘位置由 `crate::paths` 运行时解析(用户可自定义):
//!   开发期:<repo>/data/metaagent.db
//!   发布期:引导配置 config.json 里用户选定的目录
//!
//! 测试与调试用数据应放 D:\MyProject\Datas,与本库文件分离。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use rusqlite::Connection;

pub mod branches;
pub mod events;
pub mod messages;
pub mod projects;
pub mod schema;
pub mod sessions;
pub mod todos;
pub mod tool_log;
pub mod usage;

/// 全局连接句柄(Mutex 串行化写;读多写少场景够用)。
static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 数据目录路径(运行时解析,见 `crate::paths`)。
pub fn data_dir() -> PathBuf {
    crate::paths::data_dir()
}

/// 初始化:建目录、打开数据库、跑迁移。只能在 setup 阶段调一次。
pub fn init() -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir)
        .map_err(|e| format!("create data dir {} failed: {e}", dir.display()))?;

    let db_path = dir.join("metaagent.db");
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("open {} failed: {e}", db_path.display()))?;

    // SEC-2:启用 WAL(崩溃恢复更好、读不阻塞写)与 busy_timeout(避免多连接 "database is locked")。
    // 这两条在迁移前设置,保证迁移期间也生效;WAL 是数据库级持久设置,重复设置无害。
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("set journal_mode=WAL failed: {e}"))?;
    conn.pragma_update(None, "busy_timeout", "5000")
        .map_err(|e| format!("set busy_timeout failed: {e}"))?;

    // P2:版本升级前自动快照,迁移出错时用户仍有可回滚的旧库
    snapshot_before_migrate(&conn, &db_path);

    schema::migrate(&conn).map_err(|e| format!("migrate failed: {e}"))?;

    DB.set(Mutex::new(conn))
        .map_err(|_| "db already initialized".to_string())?;
    log::info!("db initialized at {}", db_path.display());
    Ok(())
}

/// 获取全局连接。在 init 之后调用。
pub fn conn() -> &'static Mutex<Connection> {
    DB.get()
        .expect("db not initialized; call db::init() in setup")
}

/// 数据库是否已初始化(供不依赖 DB 的纯逻辑提前判断,避免在测试/早期阶段 panic)。
pub fn is_initialized() -> bool {
    DB.get().is_some()
}

// ---------- 迁移前快照(P2) ----------

/// 保留的快照份数(不含当前库)。
const SNAPSHOT_KEEP: usize = 2;
/// 快照文件名前缀:`metaagent-v<旧版本>-<时间戳>.db.bak`
const SNAPSHOT_PREFIX: &str = "metaagent-v";
/// 快照文件后缀。
const SNAPSHOT_SUFFIX: &str = ".db.bak";

/// 磁盘版本落后于代码版本时,用 `VACUUM INTO` 生成一份一致性副本。
/// 失败只告警、不阻断启动 —— 快照是升级兜底,不是流程必需。
fn snapshot_before_migrate(conn: &Connection, db_path: &Path) {
    let Ok(version) = conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)) else {
        return;
    };
    // 0 = 全新库(没有可回滚的历史数据);已是最新版本则无需快照
    if version <= 0 || version >= schema::SCHEMA_VERSION {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
    let snap = db_path.with_file_name(format!(
        "{SNAPSHOT_PREFIX}{version}-{stamp}{SNAPSHOT_SUFFIX}"
    ));
    match conn.execute(
        "VACUUM INTO ?1",
        rusqlite::params![snap.to_string_lossy().as_ref()],
    ) {
        Ok(_) => {
            log::info!(
                "db snapshot before migrate(v{version}) -> {}",
                snap.display()
            );
            prune_snapshots(db_path);
        }
        Err(e) => log::warn!("db snapshot before migrate failed(继续迁移): {e}"),
    }
}

/// 列出同目录下的历史快照,按修改时间升序。
fn list_snapshots(db_path: &Path) -> Vec<(std::time::SystemTime, PathBuf)> {
    let Some(dir) = db_path.parent() else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut snaps: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(SNAPSHOT_PREFIX) && name.ends_with(SNAPSHOT_SUFFIX)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    snaps.sort_by_key(|(t, _)| *t);
    snaps
}

/// 只保留最近 [`SNAPSHOT_KEEP`] 份快照,更旧的直接删除(删除失败忽略)。
fn prune_snapshots(db_path: &Path) {
    let snaps = list_snapshots(db_path);
    if snaps.len() <= SNAPSHOT_KEEP {
        return;
    }
    for (_, path) in snaps.iter().take(snaps.len() - SNAPSHOT_KEEP) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_data() {
        let d = data_dir();
        assert_eq!(d.file_name().unwrap(), "data");
    }

    /// P2 快照:仅当磁盘版本落后时生成,且副本可独立打开并保留数据。
    #[test]
    fn snapshot_is_written_only_when_version_is_outdated() {
        let dir = std::env::temp_dir().join(format!("maf-snap-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("metaagent.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (1);")
            .unwrap();
        conn.pragma_update(None, "user_version", 10).unwrap();

        snapshot_before_migrate(&conn, &db_path);
        let snaps = list_snapshots(&db_path);
        assert_eq!(snaps.len(), 1, "版本落后时应生成一份快照");
        let backup = Connection::open(&snaps[0].1).unwrap();
        let n: i64 = backup
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "快照应包含迁移前的数据");
        drop(backup);

        // 版本已是最新 → 不再快照
        conn.pragma_update(None, "user_version", schema::SCHEMA_VERSION)
            .unwrap();
        snapshot_before_migrate(&conn, &db_path);
        assert_eq!(list_snapshots(&db_path).len(), 1);

        drop(conn);
        fs::remove_dir_all(&dir).ok();
    }
}
