//! fs::text 单元测试(读写/分页/检索/编辑/导出/分批写入)。

use std::path::PathBuf;

    use super::*;

    fn ws() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-fs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 测试专用备份根,避免污染真实数据目录。
    fn bak() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-bak-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 分批写会话是进程级全局表,且 `write_chunk_begin_in` 会顺带清扫过期会话:
    /// 并行跑测试时,别的用例 begin 会把「被回拨成过期」的会话扫掉,
    /// 令 TTL 用例拿到「不存在」而非「已超时」。相关用例统一持此锁串行执行。
    static CHUNK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn chunk_guard() -> std::sync::MutexGuard<'static, ()> {
        CHUNK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn write_read_delete_with_backup() {
        let root = ws();
        let base = bak();
        let r1 = write_text_in(&root, "docs/a.md", "第一版", &base, false).unwrap();
        assert!(r1.backup_id.is_none());
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第一版");

        let r2 = write_text_in(&root, "docs/a.md", "第二版", &base, false).unwrap();
        assert!(r2.backup_id.is_some(), "覆盖必须产生备份");
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第二版");

        let d = delete_in(&root, "docs/a.md", &base, false).unwrap();
        assert!(d.backup_id.is_some());
        assert!(!root.join("docs/a.md").exists());
    }

    #[test]
    fn rejects_binary_and_allows_parent_jump() {
        let root = ws();
        let base = bak();
        std::fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();
        assert!(read_text_in(&root, "bin.dat", 0, 100).is_err());
        // 无沙箱:`..` 可跳出工作区(写入仍自动备份)
        let escaped = root
            .parent()
            .unwrap()
            .join(format!("maf-escape-{}.txt", std::process::id()));
        let rel = format!("../{}", escaped.file_name().unwrap().to_str().unwrap());
        write_text_in(&root, &rel, "x", &base, false).expect("`..` 不再被拦截");
        assert!(escaped.exists(), "应写入工作区外:{}", escaped.display());
        std::fs::remove_file(&escaped).ok();
    }

    #[test]
    fn read_offset_pages_through_text() {
        let root = ws();
        let base = bak();
        let body: String = "0123456789".repeat(10); // 100 字符
        write_text_in(&root, "big.txt", &body, &base, false).unwrap();
        let tail = read_text_in(&root, "big.txt", 90, 50).unwrap();
        assert!(tail.contains("已跳过前 90 字符"), "got: {tail}");
        assert!(tail.ends_with("0123456789"), "got: {tail}");
        // 偏移越过文件尾 → 空正文,不报错
        assert!(read_text_in(&root, "big.txt", 500, 10).is_ok());
    }

    #[test]
    fn grep_finds_matching_lines() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "src/a.txt", "第一行\n错误:超时\n第三行", &base, false).unwrap();
        write_text_in(&root, "src/b.md", "# 说明\n无关内容", &base, false).unwrap();
        let hits = grep_in(&root, "超时", "", 50).unwrap();
        assert_eq!(hits.len(), 1, "got: {hits:?}");
        assert!(hits[0].contains("a.txt:2:"), "got: {}", hits[0]);
        // 限定子目录 + 上限
        let single = grep_in(&root, "说明", "src/b.md", 1).unwrap();
        assert_eq!(single.len(), 1);
        assert!(grep_in(&root, "  ", "", 10).is_err());
    }

    #[test]
    fn exports_docx_and_xlsx() {
        let root = ws();
        let base = bak();
        let d = export_docx_in(&root, "out/report.docx", "标题", "第一段\n第二段", &base).unwrap();
        assert!(d.bytes > 0);
        assert!(root.join("out/report.docx").exists());

        let sheets = vec![SheetInput {
            name: "数据".into(),
            rows: vec![
                vec!["名称".into(), "值".into()],
                vec!["A".into(), "1".into()],
            ],
        }];
        let x = export_xlsx_in(&root, "out/table.xlsx", &sheets, &base, false).unwrap();
        assert!(x.bytes > 0);
        assert!(root.join("out/table.xlsx").exists());
    }

    #[test]
    fn exports_docx_with_markdown_body() {
        let root = ws();
        let base = bak();
        let body = "# 一级标题\n\n## 二级标题\n\n- 要点一\n- 要点二\n\n| 名称 | 值 |\n|---|---|\n| A | 1 |\n\n```\nlet x = 1;\n```\n\n> 引用说明\n\n正文含 **粗体** 与 `code`。";
        let r = export_docx_in(&root, "out/md.docx", "文档标题", body, &base).unwrap();
        assert!(r.bytes > 0);
        assert!(root.join("out/md.docx").exists());
    }

    #[test]
    fn read_pagination_covers_every_char() {
        let root = ws();
        let base = bak();
        // 中英混排,确保分页边界经常落在多字节字符上
        let body: String = "中文内容测试ABC\n1234567890\n".repeat(300);
        write_text_in(&root, "mix.txt", &body, &base, false).unwrap();
        let mut offset = 0usize;
        let mut pages: Vec<String> = Vec::new();
        while offset < body.chars().count() {
            let page = read_text_in(&root, "mix.txt", offset, 700).unwrap();
            // 去掉分页提示后还原正文
            let page = match page.split_once('\n') {
                Some((first, rest)) if first.starts_with("…(已跳过前") => rest.to_string(),
                _ => page,
            };
            let page = match page.rsplit_once("\n…(本段已截断") {
                Some((rest, _)) => rest.to_string(),
                None => page,
            };
            offset += page.chars().count();
            pages.push(page);
        }
        let joined: String = pages.concat();
        assert_eq!(joined, body, "分页拼接必须与原文完全一致(无重复无遗漏)");
    }

    /// P11:byte_offset 续读 —— 偏移单调前进、拼接完整、截断提示带字节续读位置。
    #[test]
    fn byte_offset_pagination_covers_every_char() {
        let root = ws();
        let base = bak();
        let body: String = "中文内容测试ABC\n1234567890\n".repeat(300);
        write_text_in(&root, "mix2.txt", &body, &base, false).unwrap();
        let mut byte = 0u64;
        let mut joined = String::new();
        let mut pages = 0;
        loop {
            let (page, next) = read_text_page_in(&root, "mix2.txt", 0, 700, Some(byte)).unwrap();
            if pages == 0 {
                assert!(
                    page.contains("byte_offset="),
                    "截断提示必须给出字节续读位置, got: {page}"
                );
            }
            // 去掉截断提示,还原正文
            let page = match page.rsplit_once("\n…(本段已截断") {
                Some((rest, _)) => rest.to_string(),
                None => page,
            };
            joined.push_str(&page);
            pages += 1;
            match next {
                Some(nb) => {
                    assert!(nb > byte, "续读偏移必须前进: {byte} -> {nb}");
                    byte = nb;
                }
                None => break,
            }
            assert!(pages < 1_000, "分页未收敛");
        }
        assert_eq!(joined, body, "byte_offset 分页拼接必须与原文完全一致");
    }

    #[test]
    fn edit_replaces_unique_occurrence() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "doc.md", "标题\n旧句子\n结尾", &base, false).unwrap();
        let r = edit_text_in(&root, "doc.md", "旧句子", "新句子", false, &base, false).unwrap();
        assert_eq!(r.replaced, 1);
        assert_eq!(
            read_text_in(&root, "doc.md", 0, 100).unwrap(),
            "标题\n新句子\n结尾"
        );
        assert!(r.backup_id.is_some(), "编辑前必须备份");
    }

    #[test]
    fn edit_rejects_missing_and_ambiguous_fragment() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "doc.md", "重复\n重复\n唯一", &base, false).unwrap();
        // 未命中
        let err = edit_text_in(&root, "doc.md", "不存在", "x", false, &base, false)
            .expect_err("未命中必须报错");
        assert!(err.contains("未在"), "got: {err}");
        // 多处且未开 replace_all
        let err = edit_text_in(&root, "doc.md", "重复", "x", false, &base, false)
            .expect_err("多处必须报错");
        assert!(err.contains("出现 2 次"), "got: {err}");
        // 多处 + replace_all → 全部替换
        let r = edit_text_in(&root, "doc.md", "重复", "替换", true, &base, false).unwrap();
        assert_eq!(r.replaced, 2);
        assert_eq!(
            read_text_in(&root, "doc.md", 0, 100).unwrap(),
            "替换\n替换\n唯一"
        );
        // old 为空直接拒绝
        assert!(edit_text_in(&root, "doc.md", "", "x", false, &base, false).is_err());
    }

    #[test]
    fn chunked_write_restores_large_file_exactly() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        // 100 段 × 1.5 万字符(≈4.3MB 字节),模拟模型分段输出
        let chunk = "分批写入测试内容🙂\n".repeat(1500);
        let total_chars = chunk.chars().count() * 100;
        let st = write_chunk_begin_in(&root, "big/out.txt", false, &tmp, None).unwrap();
        assert!(write_chunk_known(&st.write_id));
        let mut last = 0u64;
        for _ in 0..100 {
            let s = write_chunk_append(&st.write_id, &chunk).unwrap();
            assert!(s.bytes > last, "累计字节必须单调递增");
            last = s.bytes;
        }
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert!(!write_chunk_known(&st.write_id), "finish 后会话必须失效");
        assert_eq!(fin.bytes, last);
        let body = std::fs::read_to_string(root.join("big/out.txt")).unwrap();
        assert_eq!(body.chars().count(), total_chars);
        assert_eq!(body, chunk.repeat(100), "分批内容必须精确还原");
        assert!(body.len() > 3 * 1024 * 1024, "测试内容需超过 3MB");
        assert!(fin.backup_id.is_none(), "新建文件无需备份");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_write_rejects_unknown_id_and_backs_up_overwrite() {
        let _g = chunk_guard();
        let root = ws();
        let base = bak();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        assert!(write_chunk_append("no-such-id", "x").is_err());
        assert!(write_chunk_finish("no-such-id").is_err());
        write_text_in(&root, "a.txt", "旧内容", &base, false).unwrap();
        let st = write_chunk_begin_in(&root, "a.txt", false, &tmp, None).unwrap();
        write_chunk_append(&st.write_id, "新内容").unwrap();
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert!(fin.backup_id.is_some(), "覆盖已有文件必须备份");
        assert_eq!(read_text_in(&root, "a.txt", 0, 100).unwrap(), "新内容");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn read_without_max_chars_returns_whole_file() {
        let root = ws();
        let base = bak();
        let body = "长文".repeat(200_000); // 40 万字符
        write_text_in(&root, "huge.txt", &body, &base, false).unwrap();
        let got = read_text_in(&root, "huge.txt", 0, usize::MAX).unwrap();
        assert_eq!(got, body, "max_chars 不设上限时应一次读完");
    }

    #[test]
    fn read_strips_bom_and_tolerates_invalid_utf8() {
        let root = ws();
        std::fs::write(root.join("bom.txt"), "\u{feff}正文AB").unwrap();
        assert_eq!(read_text_in(&root, "bom.txt", 0, 10).unwrap(), "正文AB");
        // 非法 UTF-8 字节按替换字符占位,不 panic、不中断
        std::fs::write(root.join("bad.txt"), [b'h', 0xFF, b'i']).unwrap();
        assert_eq!(
            read_text_in(&root, "bad.txt", 0, 10).unwrap(),
            "h\u{fffd}i"
        );
    }

    #[test]
    fn boundary_check_blocks_outside_write() {
        // enforce_boundary=true 时,相对 `../escape.txt` 被拦截
        let root = ws();
        let base = bak();
        let escaped = root
            .parent()
            .unwrap()
            .join(format!("maf-guard-{}.txt", std::process::id()));
        let rel = format!("../{}", escaped.file_name().unwrap().to_str().unwrap());
        let err = write_text_in(&root, &rel, "x", &base, true)
            .expect_err("越界写入必须被拦截");
        assert!(
            err.contains("不在项目工作区"),
            "错误应提示越界,实际: {err}"
        );
        assert!(!escaped.exists(), "文件不应被创建");
        // 正常项目内写入仍可成功
        let ok = write_text_in(&root, "safe.txt", "ok", &base, true);
        assert!(ok.is_ok(), "项目内写入应成功");
        // 精确编辑同样受限
        let outside = root
            .parent()
            .unwrap()
            .join(format!("maf-guard-edit-{}.txt", std::process::id()));
        std::fs::write(&outside, "outside").unwrap();
        let rel_out = format!("../{}", outside.file_name().unwrap().to_str().unwrap());
        assert!(
            edit_text_in(&root, &rel_out, "outside", "x", false, &base, true).is_err(),
            "越界编辑必须被拦截"
        );
        std::fs::remove_file(&outside).ok();
        std::fs::remove_file(&escaped).ok();
    }

    #[test]
    fn extreme_inputs_fail_gracefully() {
        let root = ws();
        // 空文件:返回空串,不是错误
        std::fs::write(root.join("empty.txt"), "").unwrap();
        assert_eq!(read_text_in(&root, "empty.txt", 0, 32_000).unwrap(), "");
        // 不存在 / 非法文件名 / 目标是目录:都必须是可读错误,不能 panic
        assert!(read_text_in(&root, "no-such.txt", 0, 10).is_err());
        assert!(read_text_in(&root, "bad?name.txt", 0, 10).is_err());
        std::fs::create_dir_all(root.join("sub")).unwrap();
        assert!(read_text_in(&root, "sub", 0, 10).is_err());
        // 路径含 NUL:拒绝而非崩溃
        assert!(read_text_in(&root, "a\u{0}b.txt", 0, 10).is_err());
    }

    #[test]
    fn chunked_write_handles_10mb_across_many_chunks() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let chunk = "A".repeat(50_000); // 每片 50KB,共 200 片 = 10MB
        let st = write_chunk_begin_in(&root, "big10.txt", false, &tmp, None).unwrap();
        for _ in 0..200 {
            write_chunk_append(&st.write_id, &chunk).unwrap();
        }
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert_eq!(fin.bytes, 10_000_000);
        let body = std::fs::read_to_string(root.join("big10.txt")).unwrap();
        assert_eq!(body.len(), 10_000_000, "10MB 内容必须完整落盘");
        assert!(body.chars().all(|c| c == 'A'));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_writes_are_isolated_across_threads() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        std::thread::scope(|s| {
            for name in ["a", "b"] {
                let root = root.clone();
                let tmp = tmp.clone();
                s.spawn(move || {
                    let st = write_chunk_begin_in(&root, &format!("{name}.txt"), false, &tmp, None)
                        .expect("begin 应成功");
                    let chunk = name.repeat(1000);
                    for _ in 0..40 {
                        write_chunk_append(&st.write_id, &chunk).expect("append 应成功");
                    }
                    write_chunk_finish(&st.write_id).expect("finish 应成功");
                    let body = std::fs::read_to_string(root.join(format!("{name}.txt"))).unwrap();
                    assert_eq!(body, name.repeat(1000).repeat(40), "并发会话不得串写");
                });
            }
        });
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_write_supports_unicode_nested_path() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let rel = "中文目录/深层结构/实验报告.md";
        let st = write_chunk_begin_in(&root, rel, false, &tmp, None).unwrap();
        let chunk = "# 标题\n正文🙂\n".repeat(100);
        for _ in 0..3 {
            write_chunk_append(&st.write_id, &chunk).unwrap();
        }
        write_chunk_finish(&st.write_id).unwrap();
        assert_eq!(
            read_text_in(&root, rel, 0, usize::MAX).unwrap(),
            chunk.repeat(3),
            "中文深层路径的分批写入必须可原样读回"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunk_begin_enforces_workspace_boundary() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let err = write_chunk_begin_in(&root, "../outside-part.txt", true, &tmp, None)
            .expect_err("越界 begin 必须被拦截");
        assert!(err.contains("不在项目工作区"), "实际: {err}");
        assert!(!root.join("../outside-part.txt").exists());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunk_session_expires_after_ttl() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let st = write_chunk_begin_in(&root, "ttl.txt", false, &tmp, None).unwrap();
        write_chunk_append(&st.write_id, "前段").unwrap();
        // 回拨到 TTL 之外
        backdate_chunk(
            &st.write_id,
            CHUNK_SESSION_TTL + std::time::Duration::from_secs(60),
        );
        let err = write_chunk_append(&st.write_id, "后段").expect_err("过期会话必须拒绝追加");
        assert!(err.contains("已超时"), "got: {err}");
        assert!(!write_chunk_known(&st.write_id), "过期会话必须失效");
        assert!(
            !tmp.join(format!("{}.part", st.write_id)).exists(),
            "过期临时分片应被清理"
        );
        assert!(write_chunk_finish(&st.write_id).is_err(), "过期会话不可收尾");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunk_sessions_per_owner_are_capped() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let sid = format!("owner-{}", uuid::Uuid::new_v4());
        let mut ids = Vec::new();
        for _ in 0..MAX_CHUNK_SESSIONS_PER_OWNER {
            let st = write_chunk_begin_in(&root, "cap.txt", false, &tmp, Some(&sid)).unwrap();
            ids.push(st.write_id);
        }
        let err = write_chunk_begin_in(&root, "cap2.txt", false, &tmp, Some(&sid))
            .expect_err("超过并发上限必须拒绝");
        assert!(err.contains("上限"), "got: {err}");
        for id in ids {
            write_chunk_finish(&id).unwrap();
        }
        // 收尾后名额释放,可以继续 begin
        let st = write_chunk_begin_in(&root, "cap3.txt", false, &tmp, Some(&sid)).unwrap();
        write_chunk_finish(&st.write_id).unwrap();
        std::fs::remove_dir_all(&tmp).ok();
    }

