//! 数据目录运行时解析(全项目唯一的路径事实源)。
//!
//! 历史问题:早期版本用编译期常量 `env!("CARGO_MANIFEST_DIR")` 拼接 `<repo>/data`,
//! 字符串被打进二进制,安装后恒为构建机的源码路径,用户无法自定义。
//!
//! 解析优先级(`init` 时确定并缓存):
//!   1. 环境变量 `MYAGENT_DATA_DIR`(调试/自动化)
//!   2. 引导配置 `config.json` 里的 `data_dir`
//!   3. 历史兼容:`<源码目录>/data` 存在且含 `metaagent.db` → 采用(老版本升级不丢数据)
//!   4. debug 构建 → `<源码目录>/data`(保持开发期行为,开发/测试数据留在 D 盘)
//!   5. 其它 → 待定目录(先让 db::init 跑起来,随后由首次启动弹窗接管)
//!
//! 引导配置放在 `%APPDATA%/<identifier>/config.json`(Tauri app_config_dir,安装版默认);
//! 仅当 `app.exe` 同目录**已存在** config.json 时视为便携模式,读写都走 exe 旁(显式 opt-in,
//! 避免 release 首启把数据静默写进安装目录)。
//!
//! 分层:
//! - 本文件:引导配置读写、数据目录解析入口与初始化/切换;
//! - `resolve.rs`:路径规范化与候选目录解析的纯逻辑;
//! - `migrate.rs`:切换目录后的数据迁移(复制 / 校验 / 附件路径重写)。

mod migrate;
mod resolve;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

use migrate::migrate_dir;
use resolve::{canon, is_writable, pick_config_dir, plain_path, resolve_data_dir, same_path};

/// 引导配置(必须独立于数据目录,否则先有鸡后有蛋)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    /// 用户选定的数据根目录(绝对路径);None = 尚未配置
    #[serde(default)]
    pub data_dir: Option<String>,
    /// 待迁移来源:切换目录后,下次启动时把该目录的数据搬到新目录
    #[serde(default)]
    pub pending_migration_from: Option<String>,
}

/// `set_data_dir` 的结果。
#[derive(Debug, Clone, Serialize)]
pub struct SetDirResult {
    /// 是否需要重启应用才能生效
    pub restart_required: bool,
    /// 待迁移/已迁移的旧目录(无则 None)
    pub old_dir: Option<String>,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
static MIGRATION_ERROR: OnceLock<String> = OnceLock::new();

// ---------------- 引导配置 ----------------

/// 引导配置目录(init 之前按 exe 旁计算)。
pub fn config_dir() -> PathBuf {
    CONFIG_DIR
        .get()
        .cloned()
        .unwrap_or_else(resolve::exe_dir)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// 读取引导配置;文件缺失/损坏一律退回默认(不报错,保证可启动)。
pub fn read_bootstrap() -> Bootstrap {
    let Ok(text) = fs::read_to_string(config_path()) else {
        return Bootstrap::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn write_bootstrap(b: &Bootstrap) -> Result<(), String> {
    let p = config_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(b).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

// ---------------- 数据目录解析 ----------------

/// 解析后的数据根目录。init 之前调用会即时解析,保证单测与早期调用可用。
pub fn data_dir() -> PathBuf {
    DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| resolve_data_dir(&read_bootstrap()))
}

/// 是否已在引导配置里显式选定过数据目录。
pub fn is_configured() -> bool {
    read_bootstrap()
        .data_dir
        .as_deref()
        .map(|d| !d.is_empty())
        .unwrap_or(false)
}

/// 是否采用了历史数据目录(未配置但已有数据)。
pub fn legacy_adopted() -> bool {
    !is_configured() && data_dir().join("metaagent.db").exists()
}

pub fn migration_error() -> Option<String> {
    MIGRATION_ERROR.get().cloned()
}

/// 初始化:确定数据目录并执行「待迁移」。必须在 `db::init()` 之前调用。
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let _ = CONFIG_DIR.set(pick_config_dir(app));

    let boot = read_bootstrap();
    let mut resolved = resolve_data_dir(&boot);

    // 上一进程已把 data_dir 指向新目录,这里把旧目录数据搬过去(此时无任何文件句柄)
    if let Some(from) = boot.pending_migration_from.as_deref().map(PathBuf::from)
        && !same_path(&from, &resolved)
    {
        match migrate_dir(&from, &resolved) {
            Ok(()) => {
                log::info!(
                    "data dir migrated: {} -> {}",
                    from.display(),
                    resolved.display()
                );
                let mut b = read_bootstrap();
                b.data_dir = Some(resolved.display().to_string());
                b.pending_migration_from = None;
                if let Err(e) = write_bootstrap(&b) {
                    log::error!("clear pending migration failed: {e}");
                }
            }
            Err(e) => {
                // 迁移失败:保留标志位,本次回退到旧目录运行,下次启动自动重试(幂等)
                log::error!("data dir migration failed: {e}");
                let _ = MIGRATION_ERROR.set(e);
                resolved = plain_path(&from);
            }
        }
    }

    fs::create_dir_all(&resolved)
        .map_err(|e| format!("create data dir {} failed: {e}", resolved.display()))?;
    DATA_DIR
        .set(resolved)
        .map_err(|_| "data dir already initialized".to_string())?;
    log::info!("data dir: {}", data_dir().display());
    Ok(())
}

/// 切换数据目录(写引导配置;迁移在下次启动、无文件句柄时执行)。
pub fn set_data_dir(new_dir: &Path, migrate: bool) -> Result<SetDirResult, String> {
    if new_dir.as_os_str().is_empty() || !new_dir.is_absolute() {
        return Err("请选择一个绝对路径的目录".into());
    }
    let cur = data_dir();

    // 与当前目录相同 = 仅确认配置,无需重启
    if same_path(&cur, new_dir) {
        let mut b = read_bootstrap();
        b.data_dir = Some(plain_path(&canon(&cur)).display().to_string());
        b.pending_migration_from = None;
        write_bootstrap(&b)?;
        return Ok(SetDirResult {
            restart_required: false,
            old_dir: None,
        });
    }

    // 新旧目录不得互相包含(防止把自己复制进自己)
    let c = canon(&cur);
    let n = canon(new_dir);
    if c.starts_with(&n) || n.starts_with(&c) {
        return Err("新旧数据目录不能互相包含,请选择其它目录".into());
    }
    // 引导配置目录不能被当作数据目录(迁移后会连同配置一起被删掉)
    let cd = canon(&config_dir());
    if n.starts_with(&cd) || cd.starts_with(&n) {
        return Err("数据目录不能与引导配置所在目录相同或互相包含".into());
    }

    fs::create_dir_all(&n).map_err(|e| format!("创建目录失败:{e}"))?;
    if !is_writable(&n) {
        return Err("该目录不可写,请另选一个".into());
    }
    if n.join("metaagent.db").exists() {
        return Err("该目录已存在 myAgent 数据(metaagent.db),请另选一个,避免覆盖".into());
    }

    let pending = if migrate && cur.exists() {
        Some(plain_path(&c).display().to_string())
    } else {
        None
    };
    let b = Bootstrap {
        data_dir: Some(plain_path(&n).display().to_string()),
        pending_migration_from: pending.clone(),
    };
    write_bootstrap(&b)?;
    Ok(SetDirResult {
        restart_required: true,
        old_dir: pending,
    })
}

#[cfg(test)]
mod tests;
