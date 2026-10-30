//! db 模块入口。
//!
//! 数据落盘位置由 `crate::paths` 运行时解析(用户可自定义):
//!   开发期:<repo>/data/metaagent.db
//!   发布期:引导配置 config.json 里用户选定的目录
//!
//! 测试与调试用数据应放 D:\MyProject\Datas,与本库文件分离。

use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use rusqlite::Connection;

pub mod branches;
pub mod projects;
pub mod messages;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_data() {
        let d = data_dir();
        assert_eq!(d.file_name().unwrap(), "data");
    }
}
