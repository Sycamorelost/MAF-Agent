//! 翻译页预置术语表(内置权威中英对照词表,与知识库笔记互不依赖)。
//!
//! 数据文件 `glossary_presets/<主题>.tsv`,编译期 `include_str!` 载入;格式:
//! - `# name<TAB>分类名` —— 分类显示名
//! - `# source<TAB>来源` —— 该表权威来源(用于「一键导入」后的溯源说明)
//! - `英文<TAB>中文<TAB>使用提示(可选)` —— 词条;中译名可用「；」并列多译名
//!
//! 对外暴露 `pairs()`:已做全局去重(按英文词小写判重)与双向展开
//! (英→中 + 中→英),直接供 `glossary_import_presets` 合并进用户术语表。

use std::collections::HashSet;

/// 一条预置术语(单方向,可直接落进 glossary.json)。
#[derive(Debug, Clone)]
pub struct PresetTermPair {
    pub src: String,
    pub dst: String,
    /// 分类显示名(如「软件/IT 开发」)
    pub cat: String,
    /// 使用提示(如「不译『提示』」),无则为空
    pub hint: Option<String>,
}

/// 全部预置表(顺序即分类顺序)。
const FILES: &[&str] = &[
    include_str!("glossary_presets/it.tsv"),
    include_str!("glossary_presets/ai.tsv"),
    include_str!("glossary_presets/academic.tsv"),
    include_str!("glossary_presets/legal.tsv"),
    include_str!("glossary_presets/finance.tsv"),
    include_str!("glossary_presets/medical.tsv"),
    include_str!("glossary_presets/business.tsv"),
    include_str!("glossary_presets/general.tsv"),
];

/// 解析后的单张表。
struct Table {
    cat: String,
    source: String,
    rows: Vec<(String, String, Option<String>)>,
}

fn parse_table(body: &str) -> Table {
    let mut t = Table {
        cat: String::new(),
        source: String::new(),
        rows: Vec::new(),
    };
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let rest = rest.trim();
            if let Some(v) = rest.strip_prefix("name") {
                t.cat = v.trim().to_string();
            } else if let Some(v) = rest.strip_prefix("source") {
                t.source = v.trim().to_string();
            }
            continue;
        }
        let cells: Vec<&str> = line.split('\t').map(|c| c.trim()).collect();
        if cells.len() < 2 || cells[0].is_empty() || cells[1].is_empty() {
            continue;
        }
        let hint = cells.get(2).map(|s| s.to_string()).filter(|s| !s.is_empty());
        t.rows.push((cells[0].to_string(), cells[1].to_string(), hint));
    }
    t
}

fn tables() -> Vec<Table> {
    FILES.iter().map(|b| parse_table(b)).collect()
}

/// 分类清单:(分类名, 权威来源),供 UI/说明展示。
pub fn categories() -> Vec<(String, String)> {
    tables().into_iter().map(|t| (t.cat, t.source)).collect()
}

/// 双向展开的预置术语:英→中(含中文多译名拆分)与中→英,按英文词全局去重。
pub fn pairs() -> Vec<PresetTermPair> {
    let mut out: Vec<PresetTermPair> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for t in tables() {
        for (en, zh, hint) in &t.rows {
            if !seen.insert(en.to_lowercase()) {
                continue; // 跨表重复以先出现者为准
            }
            out.push(PresetTermPair {
                src: en.clone(),
                dst: zh.clone(),
                cat: t.cat.clone(),
                hint: hint.clone(),
            });
            // 中文多译名(「责任；负债」)→ 各自成为中→英条目。
            // 单字中文(“库/应/其”)不进反向表:子串匹配下会在几乎每句中文里误命中,
            // 反而把无关术语注入 prompt;正向(英→中)映射保留。
            for part in zh.split(['；', ';']).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                if part.chars().count() < 2 {
                    continue;
                }
                if seen.insert(part.to_lowercase()) {
                    out.push(PresetTermPair {
                        src: part.to_string(),
                        dst: en.clone(),
                        cat: t.cat.clone(),
                        hint: hint.clone(),
                    });
                }
            }
        }
    }
    out
}

/// 预置条目总数(不含双向展开,仅英文词条),用于测试与文档。
#[allow(dead_code)]
pub fn entry_count() -> usize {
    tables().iter().map(|t| t.rows.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_well_formed() {
        let ts = tables();
        assert_eq!(ts.len(), 8, "应有 8 张预置术语表");
        for t in &ts {
            assert!(!t.cat.is_empty(), "缺 name 头");
            assert!(!t.source.is_empty(), "缺 source 头:{}", t.cat);
            assert!(t.rows.len() >= 30, "{} 词条过少:{}", t.cat, t.rows.len());
        }
        assert!(entry_count() >= 300, "总词条 {} 偏少", entry_count());
    }

    #[test]
    fn no_duplicate_english_keys() {
        let mut seen = HashSet::new();
        for t in tables() {
            for (en, _, _) in t.rows {
                assert!(seen.insert(en.to_lowercase()), "重复词条:{}", en);
            }
        }
    }

    #[test]
    fn known_authoritative_translations() {
        let p = pairs();
        let find = |src: &str| {
            p.iter()
                .find(|x| x.src == src)
                .map(|x| x.dst.clone())
                .unwrap_or_default()
        };
        assert_eq!(find("idempotent"), "幂等");
        assert_eq!(find("liquidated damages"), "违约金");
        assert_eq!(find("retrieval-augmented generation"), "检索增强生成");
        assert_eq!(find("contraindication"), "禁忌证");
        assert_eq!(find("accrual basis"), "权责发生制");
        // 反向
        assert_eq!(find("幂等"), "idempotent");
        assert_eq!(find("对价"), "consideration");
    }

    #[test]
    fn pairs_expanded_bidirectionally() {
        let p = pairs();
        assert!(p.len() > entry_count(), "双向展开后应多于原始词条");
        // 每个条目字段非空
        for x in &p {
            assert!(!x.src.trim().is_empty());
            assert!(!x.dst.trim().is_empty());
            assert!(!x.cat.trim().is_empty());
        }
    }

    #[test]
    fn no_single_char_reverse_entries() {
        // 单字中文反向词条在子串匹配下会误命中几乎每句中文,必须被排除
        let p = pairs();
        // 含非 ASCII 字符的 src(即中文反向词条)必须至少 2 个字
        assert!(
            p.iter().all(|x| x.src.is_ascii() || x.src.chars().count() >= 2),
            "出现单字中文反向词条:{:?}",
            p.iter().filter(|x| !x.src.is_ascii() && x.src.chars().count() < 2).collect::<Vec<_>>()
        );
        // 正向映射仍在
        let find = |src: &str| p.iter().find(|x| x.src == src).map(|x| x.dst.clone());
        assert_eq!(find("library").as_deref(), Some("库"));
        assert_eq!(find("库"), None);
        assert_eq!(find("其"), None);
    }

    #[test]
    fn categories_carry_sources() {
        let cs = categories();
        assert_eq!(cs.len(), 8);
        assert!(cs.iter().any(|(c, _)| c.contains("IT")));
        assert!(cs.iter().all(|(_, s)| s.contains(";") || !s.is_empty()));
    }
}
