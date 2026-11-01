//! 文件工具全链路回归:写入 → 编辑 → 读取 → 分批写 → 分页读回。

use super::execute_builtin;
use serde_json::json;

/// MCP 本地编辑全链路:fs_write → fs_edit → fs_read → 分批写 → 分页读回。
/// 参数里全部带中文,同时回归此前的多字节切片 panic。
#[tokio::test]
async fn fs_tools_edit_and_chunked_write_roundtrip() {
    let dir = std::env::temp_dir().join(format!("maf-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.md").to_str().unwrap().to_string();

    let (content, ok) = execute_builtin(
        "fs_write",
        &json!({ "path": path, "content": "标题\n旧句子\n结尾" }),
        false,
        None,
        None,
    )
    .await;
    assert!(ok, "写入失败:{content}");
    assert!(content.contains("\"bytes\""), "应返回 JSON:{content}");

    let (content, ok) = execute_builtin(
        "fs_edit",
        &json!({ "path": path, "old": "旧句子", "new": "新句子" }),
        false,
        None,
        None,
    )
    .await;
    assert!(ok, "编辑失败:{content}");
    let edited: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(edited["replaced"], 1);

    let (content, ok) = execute_builtin("fs_read", &json!({ "path": path }), false, None, None).await;
    assert!(ok, "读取失败:{content}");
    assert_eq!(content, "标题\n新句子\n结尾");

    // 分批写入:3 段 × 3.5 万字符 = 10.5 万字符(远超单次安全输出量)
    let big_path = dir.join("big.txt").to_str().unwrap().to_string();
    let (content, ok) = execute_builtin(
        "fs_write_begin",
        &json!({ "path": big_path }),
        false,
        None,
        None,
    )
    .await;
    assert!(ok, "begin 失败:{content}");
    let st: serde_json::Value = serde_json::from_str(&content).unwrap();
    let write_id = st["write_id"].as_str().unwrap().to_string();

    let chunk = "分批写入校验🙂".repeat(5000);
    let mut total = 0u64;
    for _ in 0..3 {
        let (content, ok) = execute_builtin(
            "fs_write_chunk",
            &json!({ "write_id": write_id, "content": chunk }),
            false,
            None,
            None,
        )
        .await;
        assert!(ok, "chunk 失败:{content}");
        total = serde_json::from_str::<serde_json::Value>(&content).unwrap()["bytes"]
            .as_u64()
            .unwrap();
    }
    let (content, ok) = execute_builtin(
        "fs_write_finish",
        &json!({ "write_id": write_id }),
        false,
        None,
        None,
    )
    .await;
    assert!(ok, "finish 失败:{content}");
    let fin: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(fin["bytes"].as_u64().unwrap(), total);
    assert!(fin["backup_id"].is_null(), "新建文件不应产生备份");

    // 分页读回并拼成全文,验证不丢字、不重复
    let expected = chunk.repeat(3);
    let mut joined = String::new();
    let mut offset = 0usize;
    while offset < expected.chars().count() {
        let (page, ok) = execute_builtin(
            "fs_read",
            &json!({ "path": big_path, "offset": offset, "max_chars": 50_000 }),
            false,
            None,
            None,
        )
        .await;
        assert!(ok, "读取失败:{page}");
        let body = match page.split_once('\n') {
            Some((first, rest)) if first.starts_with("…(已跳过前") => rest.to_string(),
            _ => page,
        };
        let body = match body.rsplit_once("\n…(本段已截断") {
            Some((rest, _)) => rest.to_string(),
            None => body,
        };
        offset += body.chars().count();
        joined.push_str(&body);
    }
    assert_eq!(joined, expected, "分批写入的内容必须能完整读回");
    std::fs::remove_dir_all(&dir).ok();
}
