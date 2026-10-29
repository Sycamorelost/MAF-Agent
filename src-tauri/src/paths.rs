//! 数据目录运行时解析(全项目唯一的路径事实源)。
//!
//! 历史问题:早期版本用编译期常量 `env!("CARGO_MANIFEST_DIR")` 拼接 `<repo>/data`,
//! 字符串被打进二进制,安装后恒为构建机的源码路径,用户无法自定义。
//!
//! 解析优先级(`init` 时确定并缓存):
//!   1. 环境变量 `MAF_DATA_DIR`(调试/自动化)
//!   2. 引导配置 `config.json` 里的 `data_dir`
//!   3. 历史兼容:`<源码目录>/data` 存在且含 `metaagent.db` → 采用(老版本升级不丢数据)
//!   4. debug 构建 → `<源码目录>/data`(保持开发期行为,开发/测试数据留在 D 盘)
//!   5. 其它 → 待定目录(先让 db::init 跑起来,随后由首次启动弹窗接管)
//!
//! 引导配置放在 `app.exe` 同目录(便携);该目录不可写时回退
//! `%APPDATA%/<identifier>/config.json`(Tauri app_config_dir)。

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};
use tauri::Manager;

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

// ---------------- 基础工具 ----------------

/// `app.exe` 同目录。
fn exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 尽力规范化路径:存在则 `canonicalize`;不存在则规范化最近的已存在祖先再拼回剩余分量。
///
/// 必须统一两种结果:Windows 下 `canonicalize` 会给存在的路径加 `\\?\` 前缀,而
/// 不存在的路径拿不到前缀,直接比较会让「目录互相包含」的判断失效,进而把目录
/// 复制进自己内部造成无限递归。
fn canon(p: &Path) -> PathBuf {
    if let Ok(c) = fs::canonicalize(p) {
        return c;
    }
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    while let Some(name) = cur.file_name() {
        rest.push(name.to_os_string());
        let Some(parent) = cur.parent().map(Path::to_path_buf) else {
            break;
        };
        if let Ok(c) = fs::canonicalize(&parent) {
            let mut out = c;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        if parent.as_os_str().is_empty() {
            break;
        }
        cur = parent;
    }
    p.to_path_buf()
}

/// 去尾部分隔符的小写字符串(Windows 路径大小写不敏感)。
fn key(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/// 是否同一路径。
fn same_path(a: &Path, b: &Path) -> bool {
    key(&canon(a)) == key(&canon(b))
}

/// 探测目录是否可写(写探针文件再删除)。
fn is_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(".maf-write-probe-{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// ---------------- 引导配置 ----------------

/// 引导配置目录:优先 exe 旁,不可写则回退 Tauri app_config_dir。
fn pick_config_dir(app: &tauri::AppHandle) -> PathBuf {
    let exe = exe_dir();
    if is_writable(&exe) {
        return exe;
    }
    app.path().app_config_dir().unwrap_or_else(|_| {
        env::var_os("APPDATA")
            .map(|v| PathBuf::from(v).join("com.metaagent.float"))
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

/// 引导配置目录(init 之前按 exe 旁计算)。
pub fn config_dir() -> PathBuf {
    CONFIG_DIR.get().cloned().unwrap_or_else(exe_dir)
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

/// 历史数据目录(老版本编译期固化的位置),仅用于升级兼容与开发期默认。
fn legacy_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}

// ---------------- 数据目录解析 ----------------

/// 按优先级解析数据目录(不依赖 AppHandle)。
fn resolve_data_dir(boot: &Bootstrap) -> PathBuf {
    // 1. 环境变量覆盖
    if let Some(v) = env::var_os("MAF_DATA_DIR")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    // 单测:固定用仓库 data/(与历史行为一致,避免误写用户目录)
    if cfg!(test) {
        return legacy_data_dir();
    }
    // 2. 引导配置
    if let Some(d) = boot.data_dir.as_deref()
        && !d.is_empty()
    {
        return PathBuf::from(d);
    }
    // 3. 历史兼容(老版本升级)
    let legacy = legacy_data_dir();
    if legacy.join("metaagent.db").exists() {
        return legacy;
    }
    // 4. 开发期
    if cfg!(debug_assertions) {
        return legacy;
    }
    // 5. 待定目录
    config_dir().join("data")
}

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
                resolved = from;
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
        b.data_dir = Some(canon(&cur).display().to_string());
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
        return Err("该目录已存在 MAF Agent 数据(metaagent.db),请另选一个,避免覆盖".into());
    }

    let pending = if migrate && cur.exists() {
        Some(c.display().to_string())
    } else {
        None
    };
    let b = Bootstrap {
        data_dir: Some(n.display().to_string()),
        pending_migration_from: pending.clone(),
    };
    write_bootstrap(&b)?;
    Ok(SetDirResult {
        restart_required: true,
        old_dir: pending,
    })
}

// ---------------- 目录迁移 ----------------

/// 递归复制目录(跳过符号链接)。
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("mkdir {} failed: {e}", to.display()))?;
    let rd = fs::read_dir(from).map_err(|e| format!("read {} failed: {e}", from.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("copy {} failed: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// 目录快照:相对路径(统一 `/` 分隔) → 字节数。
fn snapshot(dir: &Path) -> Result<BTreeMap<String, u64>, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) -> Result<(), String> {
        let rd = fs::read_dir(dir).map_err(|e| format!("read {} failed: {e}", dir.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk(base, &p, out)?;
            } else {
                let rel = p
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let size = entry.metadata().map_err(|e| e.to_string())?.len();
                out.insert(rel, size);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    if dir.is_dir() {
        walk(dir, dir, &mut out)?;
    }
    Ok(out)
}

/// 数据目录迁移后,把 `messages.attachments` JSON 里的绝对路径前缀从旧目录改为新目录。
/// 附件路径在库里是绝对路径;不重写的话迁移后图片/附件会全部失效(R8)。
/// 具体改写逻辑在 `doc::service::rewrite_attachment_paths`(文档生命周期统一入口),
/// 这里只负责打开库文件与容错:非 SQLite 文件(如测试占位)静默跳过,不阻断迁移。
fn rewrite_attachment_prefix(db: &Path, old_root: &Path, new_root: &Path) -> Result<usize, String> {
    if !db.is_file() {
        return Ok(0);
    }
    let conn = match rusqlite::Connection::open(db) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("skip attachment path rewrite, open db failed: {e}");
            return Ok(0);
        }
    };
    let old = old_root.to_string_lossy().to_string();
    let new = new_root.to_string_lossy().to_string();
    match crate::doc::service::rewrite_attachment_paths(&conn, &old, &new) {
        Ok(n) => Ok(n),
        Err(e) => {
            log::warn!("skip attachment path rewrite, rewrite failed: {e}");
            Ok(0)
        }
    }
}

/// 把 `from` 的数据迁移到 `to`:复制 → 校验 → 重写附件路径 → 删除旧目录。幂等,可重试。
fn migrate_dir(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Ok(());
    }
    let cf = canon(from);
    let ct = canon(to);
    if cf.starts_with(&ct) || ct.starts_with(&cf) {
        return Err("新旧目录互相包含,已中止迁移".into());
    }
    if to.join("metaagent.db").exists() && from.join("metaagent.db").exists() {
        return Err("目标目录已存在数据库文件,已中止迁移".into());
    }

    copy_tree(from, to)?;

    // 校验:源目录每个文件都在目标目录里且大小一致
    let src = snapshot(from)?;
    let dst = snapshot(to)?;
    for (rel, size) in &src {
        match dst.get(rel) {
            Some(s) if s == size => {}
            _ => return Err(format!("迁移校验失败,文件缺失或大小不一致:{rel}")),
        }
    }

    // 附件是绝对路径:迁移后必须重写,否则旧目录一删附件就全部失效
    let rewritten = rewrite_attachment_prefix(&to.join("metaagent.db"), from, to)?;
    if rewritten > 0 {
        log::info!("rewrote attachment paths for {rewritten} messages");
    }

    // 旧目录保护:绝不删除引导配置所在目录或其祖先
    let cp = canon(&config_path());
    if !cp.starts_with(&cf) {
        fs::remove_dir_all(from).map_err(|e| format!("删除旧目录失败:{e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试临时目录(放 target/ 下,留在 D 盘,不污染数据目录)。
    fn tmp(case: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join("paths")
            .join(case);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn data_dir_is_named_data_in_tests() {
        // 单测固定使用仓库 data/,保持与历史行为一致
        assert_eq!(data_dir().file_name().unwrap(), "data");
    }

    #[test]
    fn same_path_ignores_trailing_sep_and_case() {
        let d = tmp("same_path");
        assert!(same_path(&d, &d));
        assert!(same_path(&d, &PathBuf::from(format!("{}\\", d.display()))));
    }

    #[test]
    fn migrate_copies_and_deletes_old_dir() {
        let base = tmp("migrate_ok");
        let from = base.join("old");
        let to = base.join("new");
        fs::create_dir_all(from.join("conversations").join("abc")).unwrap();
        fs::write(from.join("metaagent.db"), b"db-bytes").unwrap();
        fs::write(
            from.join("conversations").join("abc").join("memory.md"),
            b"# mem",
        )
        .unwrap();
        fs::write(from.join("settings.json"), b"{}").unwrap();

        migrate_dir(&from, &to).unwrap();

        assert!(!from.exists(), "旧目录应被删除");
        assert_eq!(fs::read(to.join("metaagent.db")).unwrap(), b"db-bytes");
        assert_eq!(
            fs::read(to.join("conversations").join("abc").join("memory.md")).unwrap(),
            b"# mem"
        );
        assert!(to.join("settings.json").exists());
    }

    #[test]
    fn migrate_rewrites_attachment_paths() {
        let base = tmp("migrate_attachments");
        let from = base.join("old");
        let to = base.join("new");
        fs::create_dir_all(from.join("attachments")).unwrap();
        let old_path = format!("{}\\attachments\\a.pdf", from.display());
        {
            let conn = rusqlite::Connection::open(from.join("metaagent.db")).unwrap();
            conn.execute_batch("CREATE TABLE messages (id INTEGER PRIMARY KEY, attachments TEXT);")
                .unwrap();
            let json = serde_json::json!([
                {"name": "a.pdf", "path": old_path, "kind": "pdf", "doc_id": "abc"}
            ])
            .to_string();
            conn.execute(
                "INSERT INTO messages (id, attachments) VALUES (1, ?1)",
                [json],
            )
            .unwrap();
        }

        migrate_dir(&from, &to).unwrap();

        let conn = rusqlite::Connection::open(to.join("metaagent.db")).unwrap();
        let raw: String = conn
            .query_row("SELECT attachments FROM messages WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let list: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        let p = list[0]["path"].as_str().unwrap();
        assert!(
            p.starts_with(&to.to_string_lossy().to_string()),
            "附件路径未改写到新目录: {p}"
        );
        assert!(p.ends_with("attachments\\a.pdf"), "路径尾部被破坏: {p}");
    }

    #[test]
    fn migrate_refuses_nested_dirs() {
        let base = tmp("migrate_nested");
        let from = base.join("old");
        let to = from.join("inner");
        fs::create_dir_all(&from).unwrap();
        assert!(migrate_dir(&from, &to).is_err());
    }

    #[test]
    fn migrate_refuses_when_target_has_db() {
        let base = tmp("migrate_target_db");
        let from = base.join("old");
        let to = base.join("new");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("metaagent.db"), b"a").unwrap();
        fs::write(to.join("metaagent.db"), b"b").unwrap();
        assert!(migrate_dir(&from, &to).is_err());
    }

    #[test]
    fn migrate_is_noop_when_old_missing() {
        let base = tmp("migrate_missing");
        assert!(migrate_dir(&base.join("none"), &base.join("new")).is_ok());
    }

    #[test]
    fn bootstrap_roundtrip() {
        let b = Bootstrap {
            data_dir: Some("D:\\MAFData".into()),
            pending_migration_from: Some("C:\\old".into()),
        };
        let text = serde_json::to_string(&b).unwrap();
        let back: Bootstrap = serde_json::from_str(&text).unwrap();
        assert_eq!(back.data_dir.as_deref(), Some("D:\\MAFData"));
        assert_eq!(back.pending_migration_from.as_deref(), Some("C:\\old"));
        // 旧版可能只有 data_dir 字段
        let only: Bootstrap = serde_json::from_str("{\"data_dir\":\"D:\\\\x\"}").unwrap();
        assert!(only.pending_migration_from.is_none());
    }
}
