//! 知识库 / 笔记库命令(迭代 D8)。
//!
//! 定位见 plan/05 §12:知识库 = 用户自建 Markdown 笔记 + FTS5 全文检索,
//! 与 memory.md 并列的第二种「文件即事实源」。
//!   - 落盘:`<repo>/data/notes/{8位hex}.md`,YAML front-matter + Markdown 正文
//!   - 索引:`notes` 元数据表 + `notes_fts`(trigram 分词,与 messages_fts 同口径)
//!   - 外部编辑 md 后,notes_list / notes_get 重新读盘即生效(文件为准)

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::memory::new_session_id;

/// 笔记元数据(列表项,不含正文)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    /// 正文摘要(列表预览,前 120 字符)
    pub excerpt: String,
}

/// 笔记完整内容(notes_get 返回)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteDoc {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    pub body: String,
}

/// 笔记文件的 YAML front-matter。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NoteFrontmatter {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    created: String,
    #[serde(default)]
    updated: String,
}

/// 笔记目录:`<repo>/data/notes`。
fn notes_dir() -> PathBuf {
    db::data_dir().join("notes")
}

/// 标签序列化(逗号分隔,存储友好)。
fn tags_to_db(tags: &[String]) -> String {
    tags.iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

/// 正文摘要:取前 120 字符并折叠换行。
fn excerpt_of(body: &str) -> String {
    let s: String = body.trim_start().chars().take(120).collect();
    s.replace('\n', " ").trim().to_string()
}

/// 解析 md 文本 → (front-matter, body)。无 front-matter 时给默认元数据。
fn parse_note(text: &str) -> (NoteFrontmatter, String) {
    let t = text.trim_start_matches('\u{feff}');
    if !t.starts_with("---") {
        return (NoteFrontmatter::default(), text.to_string());
    }
    let rest = &t[3..];
    let Some(end) = rest.find("\n---") else {
        return (NoteFrontmatter::default(), text.to_string());
    };
    let fm: NoteFrontmatter = serde_yaml::from_str(&rest[..end]).unwrap_or_default();
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']).to_string();
    (fm, body)
}

/// 序列化 front-matter + body 为完整 md 文本。
fn render_note(fm: &NoteFrontmatter, body: &str) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    let yaml = yaml.trim_end();
    let yaml = yaml.strip_suffix("---").unwrap_or(yaml).trim_end();
    format!("---\n{}\n---\n\n{}\n", yaml, body.trim_start().trim_end())
}

/// 按 id 写入/更新 DB 元数据 + 刷新 FTS(幂等 upsert,rid 稳定)。
fn index_note(id: &str, title: &str, tags: &str, path: &str, created: &str, updated: &str, body: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    // M-13:元数据 upsert 与 FTS 刷新同事务,避免 rid 查询后索引半成品
    // unchecked_transaction:全局 db Mutex 保证同一连接无并发使用
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO notes(id, title, tags, path, created, updated, body)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET
           title=excluded.title, tags=excluded.tags, path=excluded.path,
           updated=excluded.updated, body=excluded.body",
        params![id, title, tags, path, created, updated, body],
    )
    .map_err(|e| e.to_string())?;
    let rid: i64 = tx
        .query_row("SELECT rid FROM notes WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let content = format!("{title}\n{body}");
    if let Err(e) = tx.execute("DELETE FROM notes_fts WHERE rowid=?1", [rid]) {
        log::warn!("notes_fts 旧索引删除失败(rid={rid}): {e}");
    }
    if let Err(e) = tx.execute(
        "INSERT INTO notes_fts(rowid, content) VALUES(?1,?2)",
        params![rid, content],
    ) {
        log::warn!("notes_fts 索引失败(rid={rid}): {e}");
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 从文件路径读取一条笔记(外部编辑即时生效);文件缺失返回 None。
fn read_note_file(path: &PathBuf) -> Option<(NoteFrontmatter, String)> {
    let raw = fs::read_to_string(path).ok()?;
    Some(parse_note(&raw))
}

/// 列出全部笔记(扫盘 + 重建索引,返回按更新时间倒序的元数据)。
#[tauri::command]
pub fn notes_list() -> Result<Vec<NoteMeta>, String> {
    let dir = notes_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;

    let mut metas: Vec<NoteMeta> = Vec::new();
    let mut found: HashSet<String> = HashSet::new();

    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some((fm, body)) = read_note_file(&path) else {
            continue;
        };
        let id = if fm.id.trim().is_empty() { stem.to_string() } else { fm.id.clone() };
        let title = if fm.title.trim().is_empty() {
            // 无标题时用正文首个非空行兜底
            body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().trim_start_matches('#').trim().to_string()
        } else {
            fm.title.clone()
        };
        let created = fm.created.clone();
        let updated = fm.updated.clone();
        let path_str = path.to_string_lossy().to_string();
        let tags = fm.tags.clone();
        index_note(&id, &title, &tags_to_db(&tags), &path_str, &created, &updated, &body)?;
        found.insert(id.clone());
        metas.push(NoteMeta {
            id,
            title,
            tags,
            created,
            updated,
            excerpt: excerpt_of(&body),
        });
    }

    // 清理文件已不存在的索引记录(FTS + 元数据)
    purge_missing(&found)?;

    metas.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(metas)
}

/// 删除 DB 中文件已不存在的笔记记录。
fn purge_missing(found: &HashSet<String>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT rid, id FROM notes")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (rid, id) in rows {
        if !found.contains(&id) {
            let _ = conn.execute("DELETE FROM notes_fts WHERE rowid=?1", [rid]);
            let _ = conn.execute("DELETE FROM notes WHERE rid=?1", [rid]);
        }
    }
    Ok(())
}

/// 读取单条笔记完整内容(以文件为准)。
#[tauri::command]
pub fn notes_get(id: String) -> Result<NoteDoc, String> {
    crate::memory::validate_id(&id)?;
    let path = notes_dir().join(format!("{id}.md"));
    let (fm, body) = read_note_file(&path).ok_or_else(|| format!("笔记不存在: {id}"))?;
    Ok(NoteDoc {
        id,
        title: fm.title,
        tags: fm.tags,
        created: fm.created,
        updated: fm.updated,
        body,
    })
}

/// 新建或覆盖保存笔记(id 为空 = 新建)。返回保存后的完整内容。
#[tauri::command]
pub fn notes_save(
    id: Option<String>,
    title: String,
    tags: Vec<String>,
    body: String,
) -> Result<NoteDoc, String> {
    let dir = notes_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;

    let is_new = id.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true);
    let note_id = if is_new {
        new_session_id()
    } else {
        let v = id.ok_or("缺少笔记 id")?.trim().to_string();
        crate::memory::validate_id(&v)?;
        v
    };
    let path = dir.join(format!("{note_id}.md"));

    let now = chrono::Local::now().to_rfc3339();
    // 保留原 created(文件优先,其次沿用本次新建时间)
    let created = if is_new {
        now.clone()
    } else {
        read_note_file(&path)
            .map(|(fm, _)| fm.created)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| now.clone())
    };

    let fm = NoteFrontmatter {
        id: note_id.clone(),
        title: title.trim().to_string(),
        tags: tags.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        created: created.clone(),
        updated: now.clone(),
    };
    let text = render_note(&fm, &body);
    fs::write(&path, text).map_err(|e| format!("write {} failed: {e}", path.display()))?;

    index_note(
        &note_id,
        &fm.title,
        &tags_to_db(&fm.tags),
        &path.to_string_lossy(),
        &created,
        &now,
        &body,
    )?;

    Ok(NoteDoc {
        id: note_id,
        title: fm.title,
        tags: fm.tags,
        created,
        updated: now,
        body,
    })
}

/// 删除笔记(同时清理 md 文件与 FTS 索引)。
#[tauri::command]
pub fn notes_delete(id: String) -> Result<(), String> {
    crate::memory::validate_id(&id)?;
    let path = notes_dir().join(format!("{id}.md"));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("remove {} failed: {e}", path.display()))?;
    }
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    if let Ok(rid) = conn.query_row("SELECT rid FROM notes WHERE id=?1", [&id], |r| r.get::<_, i64>(0)) {
        let _ = conn.execute("DELETE FROM notes_fts WHERE rowid=?1", [rid]);
        let _ = conn.execute("DELETE FROM notes WHERE rid=?1", [rid]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_render_roundtrip() {
        let fm = NoteFrontmatter {
            id: "abcd1234".into(),
            title: "记忆术".into(),
            tags: vec!["学习".into(), "方法".into()],
            created: "2026-10-14T10:00:00+08:00".into(),
            updated: "2026-10-14T10:00:00+08:00".into(),
        };
        let text = render_note(&fm, "# 标题\n\n正文内容");
        let (fm2, body) = parse_note(&text);
        assert_eq!(fm2.id, "abcd1234");
        assert_eq!(fm2.title, "记忆术");
        assert_eq!(fm2.tags, vec!["学习".to_string(), "方法".to_string()]);
        assert!(body.contains("正文内容"));
    }

    #[test]
    fn parse_without_frontmatter_keeps_body() {
        let (fm, body) = parse_note("# 裸正文");
        assert!(fm.id.is_empty());
        assert_eq!(body, "# 裸正文");
    }

    #[test]
    fn excerpt_collapses_newlines() {
        let e = excerpt_of("第一行\n第二行");
        assert_eq!(e, "第一行 第二行");
    }

    #[test]
    fn tags_to_db_serializes() {
        let raw = tags_to_db(&["a".into(), " b ".into(), "".into()]);
        assert_eq!(raw, "a,b");
    }
}
