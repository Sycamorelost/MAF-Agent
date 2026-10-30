//! 全局搜索命令:会话标题 + 消息全文(FTS5)+ memory.md 内容。

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::db::{self, sessions};

#[derive(Debug, Serialize)]
pub struct SessionHit {
    pub id: String,
    pub title: String,
    pub updated: String,
}

#[derive(Debug, Serialize)]
pub struct MessageHit {
    pub session_id: String,
    pub session_title: String,
    pub message_id: i64,
    pub role: String,
    pub snippet: String,
    pub created: String,
    /// 消息所属分支(V9;跳转时若与会话激活分支不同,前端先切换再定位)
    pub branch_id: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryHit {
    pub session_id: String,
    pub session_title: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct NoteHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub updated: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResults {
    pub sessions: Vec<SessionHit>,
    pub messages: Vec<MessageHit>,
    pub memory: Vec<MemoryHit>,
    pub notes: Vec<NoteHit>,
}

/// 会话目录根:`<data_dir>/conversations`
fn conversations_root() -> PathBuf {
    crate::paths::data_dir().join("conversations")
}

/// 全文搜索。消息走 FTS5(短语匹配,unicode61 分词对中文按字切分,天然支持子串命中)。
#[tauri::command]
pub async fn search_all(query: String) -> Result<SearchResults, String> {
    let q = query.trim().to_string();
    let mut out = SearchResults {
        sessions: Vec::new(),
        messages: Vec::new(),
        memory: Vec::new(),
        notes: Vec::new(),
    };
    if q.is_empty() {
        return Ok(out);
    }
    let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));

    // 1. 会话标题 / id
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, updated FROM sessions
                 WHERE title LIKE ?1 ESCAPE '\\' OR id LIKE ?1 ESCAPE '\\'
                 ORDER BY updated DESC LIMIT 8",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&like], |r| {
                Ok(SessionHit {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    updated: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for r in rows {
            out.sessions.push(r.map_err(|e| e.to_string())?);
        }
    }

    // 2. 消息全文。FTS5 trigram:查询经 3-gram OR 展开(retrieval::fts_match_query),
    //    自然语言问句不再整句 phrase(召回为零的病症,P7);无法展开(如 2 字词)回退 LIKE。
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        if let Some(expr) = crate::retrieval::fts_match_query(&q) {
            let mut stmt = conn
                .prepare(
                    "SELECT m.session_id, COALESCE(s.title,''), m.id, m.role,
                            snippet(messages_fts, 0, '', '', '…', 24) AS snip, m.created, m.branch_id
                     FROM messages_fts
                     JOIN messages m ON m.id = messages_fts.rowid
                     LEFT JOIN sessions s ON s.id = m.session_id
                     WHERE messages_fts MATCH ?1
                     ORDER BY rank, m.id DESC LIMIT 20",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([&expr], |r| {
                    Ok(MessageHit {
                        session_id: r.get(0)?,
                        session_title: r.get(1)?,
                        message_id: r.get(2)?,
                        role: r.get(3)?,
                        snippet: r.get::<_, String>(4)?.replace('\n', " "),
                        created: r.get(5)?,
                        branch_id: r.get(6)?,
                    })
                })
                .map_err(|e| format!("FTS 查询失败: {e}"))?;
            for r in rows {
                out.messages.push(r.map_err(|e| e.to_string())?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT m.session_id, COALESCE(s.title,''), m.id, m.role,
                            substr(m.content, MAX(1, instr(lower(m.content), ?1) - 30), 80) AS snip,
                            m.created, m.branch_id
                     FROM messages m
                     LEFT JOIN sessions s ON s.id = m.session_id
                     WHERE instr(lower(m.content), ?1) > 0
                     ORDER BY m.id DESC LIMIT 20",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([&q.to_lowercase()], |r| {
                    Ok(MessageHit {
                        session_id: r.get(0)?,
                        session_title: r.get(1)?,
                        message_id: r.get(2)?,
                        role: r.get(3)?,
                        snippet: r.get::<_, String>(4)?.replace('\n', " "),
                        created: r.get(5)?,
                        branch_id: r.get(6)?,
                    })
                })
                .map_err(|e| format!("LIKE 查询失败: {e}"))?;
            for r in rows {
                out.messages.push(r.map_err(|e| e.to_string())?);
            }
        }
    }

    // 3. memory.md 内容(扫盘,大小写不敏感,每会话取首个命中片段)
    {
        let root = conversations_root();
        if root.exists() {
            let q_lower = q.to_lowercase();
            let mut hits = Vec::new();
            if let Ok(entries) = fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let dir = entry.path();
                    if !dir.is_dir() {
                        continue;
                    }
                    let md = dir.join("memory.md");
                    let Ok(text) = fs::read_to_string(&md) else {
                        continue;
                    };
                    let lower = text.to_lowercase();
                    let Some(byte_idx) = lower.find(&q_lower) else {
                        continue;
                    };
                    // 按字符截取命中前后各 40 字符
                    let char_idx = lower[..byte_idx].chars().count();
                    let chars: Vec<char> = text.chars().collect();
                    let start = char_idx.saturating_sub(40);
                    let end = (char_idx + q.chars().count() + 40).min(chars.len());
                    let mut snippet: String = chars[start..end].iter().collect();
                    snippet = snippet.replace('\n', " ").trim().to_string();
                    if start > 0 {
                        snippet = format!("…{snippet}");
                    }
                    if end < chars.len() {
                        snippet = format!("{snippet}…");
                    }
                    let session_id = entry.file_name().to_string_lossy().to_string();
                    let title = sessions::get(&session_id)
                        .ok()
                        .flatten()
                        .map(|s| s.title)
                        .unwrap_or_default();
                    hits.push(MemoryHit {
                        session_id,
                        session_title: title,
                        snippet,
                    });
                    if hits.len() >= 10 {
                        break;
                    }
                }
            }
            out.memory = hits;
        }
    }

    // 4. 笔记正文(迭代 D8 / P7:notes_fts trigram,3-gram OR 展开;无法展开回退 LIKE)
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        if let Some(expr) = crate::retrieval::fts_match_query(&q) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT n.id, n.title, snippet(notes_fts, 0, '', '', '…', 24) AS snip, n.updated
                 FROM notes_fts
                 JOIN notes n ON n.rid = notes_fts.rowid
                 WHERE notes_fts MATCH ?1
                 ORDER BY rank, n.updated DESC LIMIT 10",
            ) && let Ok(rows) = stmt.query_map([&expr], |r| {
                Ok(NoteHit {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    snippet: r.get::<_, String>(2)?.replace('\n', " "),
                    updated: r.get(3)?,
                })
            }) {
                for r in rows.flatten() {
                    out.notes.push(r);
                }
            }
        } else if let Ok(mut stmt) = conn.prepare(
            "SELECT id, title,
                    substr(body, MAX(1, instr(lower(body), ?1) - 30), 80) AS snip,
                    updated
             FROM notes
             WHERE instr(lower(title), ?1) > 0 OR instr(lower(body), ?1) > 0
             ORDER BY updated DESC LIMIT 10",
        ) && let Ok(rows) = stmt.query_map([&q.to_lowercase()], |r| {
            Ok(NoteHit {
                id: r.get(0)?,
                title: r.get(1)?,
                snippet: r.get::<_, String>(2)?.replace('\n', " "),
                updated: r.get(3)?,
            })
        }) {
            for r in rows.flatten() {
                out.notes.push(r);
            }
        }
    }

    Ok(out)
}
