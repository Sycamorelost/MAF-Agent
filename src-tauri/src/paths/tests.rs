//! 数据目录解析与迁移回归测试。

use super::migrate::migrate_dir;
use super::resolve::{plain_path, same_path};
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
fn plain_path_strips_win32_extended_prefix() {
    // 配置里可能已经存了带 `\\?\` 的历史值;解析后必须回到普通形式
    assert_eq!(
        plain_path(Path::new(r"\\?\D:\MeatAgentFloat\mydata")),
        PathBuf::from(r"D:\MeatAgentFloat\mydata")
    );
    // UNC 共享路径还原成 `\\server\share`,而不是保留 `\\?\UNC\`
    assert_eq!(
        plain_path(Path::new(r"\\?\UNC\srv\share\data")),
        PathBuf::from(r"\\srv\share\data")
    );
    // 普通路径原样返回
    let p = PathBuf::from(r"D:\plain\dir");
    assert_eq!(plain_path(&p), p);
}

#[test]
fn bootstrap_roundtrip() {
    // 纯字符串往返,不触盘;夹具路径用中性占位(不再硬编码品牌历史目录)
    let b = Bootstrap {
        data_dir: Some("C:\\tmp\\maf-fixture".into()),
        pending_migration_from: Some("C:\\old".into()),
    };
    let text = serde_json::to_string(&b).unwrap();
    let back: Bootstrap = serde_json::from_str(&text).unwrap();
    assert_eq!(back.data_dir.as_deref(), Some("C:\\tmp\\maf-fixture"));
    assert_eq!(back.pending_migration_from.as_deref(), Some("C:\\old"));
    // 旧版可能只有 data_dir 字段
    let only: Bootstrap = serde_json::from_str("{\"data_dir\":\"D:\\\\x\"}").unwrap();
    assert!(only.pending_migration_from.is_none());
}
