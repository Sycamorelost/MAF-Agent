//! 中文「AI 味」词表(R3 B1)。
//!
//! 单一真源 = 内嵌 `DEFAULT_PATTERNS_JSON`;`resources/aiflavor/patterns.json` 存在且
//! 合法时覆盖(词表外置可热更,val.md §5.3)。解析失败静默回退内嵌,不影响检测可用性。

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// 内嵌词表(与 resources/aiflavor/patterns.json 内容一致)。
const DEFAULT_PATTERNS_JSON: &str = include_str!("../../resources/aiflavor/patterns.json");

/// 词表结构(外置 JSON 与本结构一一对应)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternFile {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub cliche: Vec<String>,
    #[serde(default)]
    pub connectives: Vec<String>,
    #[serde(default)]
    pub hype: Vec<String>,
}

fn one() -> u32 {
    1
}

/// 运行时词表(已去重、去空白)。
#[derive(Debug, Clone)]
pub struct Lexicon {
    pub cliche: Vec<String>,
    pub connectives: Vec<String>,
    pub hype: Vec<String>,
}

impl PatternFile {
    /// 收敛为运行时词表:去空白、去空串、去重(保序)。
    fn into_lexicon(self) -> Lexicon {
        fn clean(v: Vec<String>) -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for s in v {
                let t = s.trim().to_string();
                if t.is_empty() || out.iter().any(|x| x == &t) {
                    continue;
                }
                out.push(t);
            }
            out
        }
        Lexicon {
            cliche: clean(self.cliche),
            connectives: clean(self.connectives),
            hype: clean(self.hype),
        }
    }
}

/// 解析词表文本;空词表视为非法(避免外置文件被误清空后检测失效)。
fn parse(text: &str) -> Option<Lexicon> {
    let f: PatternFile = serde_json::from_str(text).ok()?;
    let lex = f.into_lexicon();
    if lex.cliche.is_empty() && lex.connectives.is_empty() {
        return None;
    }
    Some(lex)
}

/// 读取外置词表(缺失/损坏返回 None)。
fn load_external() -> Option<Lexicon> {
    let p = super::dir().join("patterns.json");
    let text = std::fs::read_to_string(p).ok()?;
    parse(&text)
}

/// 全局词表:首次访问解析并缓存(外置优先,内嵌兜底)。
pub fn load() -> &'static Lexicon {
    static LEX: OnceLock<Lexicon> = OnceLock::new();
    LEX.get_or_init(|| {
        load_external()
            .or_else(|| parse(DEFAULT_PATTERNS_JSON))
            .unwrap_or_else(|| Lexicon {
                cliche: Vec::new(),
                connectives: Vec::new(),
                hype: Vec::new(),
            })
    })
}

/// 统计 `needle` 在文本中出现的次数(不重叠)。
pub fn count_occ(text: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut n = 0;
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        n += 1;
        from += i + needle.len();
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_json_parses() {
        let lex = parse(DEFAULT_PATTERNS_JSON).expect("内嵌词表必须可解析");
        assert!(
            lex.cliche.len() >= 120,
            "套话词表应 ≥120,实际 {}",
            lex.cliche.len()
        );
        assert!(!lex.connectives.is_empty());
        assert!(!lex.hype.is_empty());
    }

    #[test]
    fn no_duplicates_after_clean() {
        let lex = parse(DEFAULT_PATTERNS_JSON).unwrap();
        for list in [&lex.cliche, &lex.connectives, &lex.hype] {
            let mut sorted = list.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "词表存在重复项");
        }
    }

    #[test]
    fn external_override_shape() {
        // 外置文件解析路径:合法的自定义词表可覆盖
        let custom = r#"{"version":2,"cliche":["综上所述","总之","总之"],
            "connectives":["因此"],"hype":["赋能"]}"#;
        let lex = parse(custom).unwrap();
        assert_eq!(lex.cliche.len(), 2, "重复项应被去掉");
        // 空词表视为非法,回退内嵌
        assert!(parse(r#"{"version":1,"cliche":[],"connectives":[]}"#).is_none());
        // 非法 JSON 视为非法
        assert!(parse("not json").is_none());
    }

    #[test]
    fn count_occ_counts_non_overlapping() {
        assert_eq!(count_occ("哈哈哈", "哈哈"), 1);
        assert_eq!(count_occ("赋能闭环赋能", "赋能"), 2);
        assert_eq!(count_occ("abc", ""), 0);
        assert_eq!(count_occ("abc", "z"), 0);
    }

    #[test]
    fn runtime_lexicon_loaded() {
        let lex = load();
        assert!(lex.cliche.iter().any(|w| w == "综上所述"));
    }
}
