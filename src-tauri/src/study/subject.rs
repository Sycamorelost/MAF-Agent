//! 学科归一:把模型识别出的自由文本学科名归一到内置附录 key。
//!
//! - 附录 key 集合固定:math / english / physics / chinese / programming / other;
//! - 归一仅用于选择提示词附录与题型侧重,不回写用户原文;
//! - 未命中任何关键词即 `other` → 走通用模板,行为与旧版完全一致。

/// 内置附录 key(学科附录常量按此索引)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKey {
    Math,
    English,
    Physics,
    Chinese,
    Programming,
    Other,
}

impl SubjectKey {
    /// 稳定字符串形态(落库 / 拼提示词用)。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Math => "math",
            Self::English => "english",
            Self::Physics => "physics",
            Self::Chinese => "chinese",
            Self::Programming => "programming",
            Self::Other => "other",
        }
    }
}

/// 关键词归一表:key = 附录 key,value = 命中关键词(小写子串匹配)。
/// 顺序即优先级:前面的学科先匹配(如「数学建模编程」归 math)。
const KEYWORDS: &[(&str, &str, &[&str])] = &[
    (
        "math",
        "数学",
        &[
            "数学", "math", "高数", "微积分", "线性代数", "代数", "几何", "算术", "概率论", "数理统计",
            "三角函数", "解析几何",
        ],
    ),
    (
        "english",
        "英语",
        &[
            "英语", "english", "雅思", "托福", "ielts", "toefl", "四级", "六级", "四六级", "cet",
            "商务英语", "英语口语",
        ],
    ),
    (
        "physics",
        "物理",
        &["物理", "physics", "力学", "电磁学", "热学", "光学", "量子"],
    ),
    (
        "chinese",
        "语文",
        &["语文", "文言文", "古诗词", "作文", "阅读理解", "汉语", "中文写作"],
    ),
    (
        "programming",
        "编程",
        &[
            "编程", "程序设计", "软件开发", "前端", "后端", "全栈", "python", "java", "javascript",
            "typescript", "rust", "c++", "c#", "golang", "go语言", "swift", "kotlin", "php",
            "数据结构", "算法", "leetcode", "网页开发", "app开发",
        ],
    ),
];

/// 自由文本学科名 → 附录 key。空串 / 未命中 → Other。
pub fn normalize_subject(raw: &str) -> SubjectKey {
    let s = raw.trim().to_lowercase();
    if s.is_empty() || s == "不确定" || s == "unknown" || s == "other" {
        return SubjectKey::Other;
    }
    for (key, _, words) in KEYWORDS {
        if words.iter().any(|w| s.contains(w)) {
            return match *key {
                "math" => SubjectKey::Math,
                "english" => SubjectKey::English,
                "physics" => SubjectKey::Physics,
                "chinese" => SubjectKey::Chinese,
                "programming" => SubjectKey::Programming,
                _ => SubjectKey::Other,
            };
        }
    }
    SubjectKey::Other
}

/// 归一结果的展示名(中文标签,界面/事件用)。
pub fn subject_label(key: &str) -> &'static str {
    match key {
        "math" => "数学",
        "english" => "英语",
        "physics" => "物理",
        "chinese" => "语文",
        "programming" => "编程",
        _ => "通用",
    }
}

/// 学段合法值集合;非法值归空(不阻断,仅不展示)。
pub fn normalize_grade_band(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "primary" | "小学" => "primary".into(),
        "junior" | "初中" => "junior".into(),
        "senior" | "高中" => "senior".into(),
        "university" | "大学" | "本科" => "university".into(),
        "adult" | "成人" | "在职" => "adult".into(),
        _ => String::new(),
    }
}

/// 学段展示名。
pub fn grade_band_label(band: &str) -> &'static str {
    match band {
        "primary" => "小学",
        "junior" => "初中",
        "senior" => "高中",
        "university" => "大学",
        "adult" => "成人",
        _ => "未指定",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_common_subjects() {
        assert_eq!(normalize_subject("数学"), SubjectKey::Math);
        assert_eq!(normalize_subject("高中数学"), SubjectKey::Math);
        assert_eq!(normalize_subject("Math"), SubjectKey::Math);
        assert_eq!(normalize_subject("英语"), SubjectKey::English);
        assert_eq!(normalize_subject("大学英语四级"), SubjectKey::English);
        assert_eq!(normalize_subject("物理"), SubjectKey::Physics);
        assert_eq!(normalize_subject("语文"), SubjectKey::Chinese);
        assert_eq!(normalize_subject("Python 编程"), SubjectKey::Programming);
        assert_eq!(normalize_subject("rust"), SubjectKey::Programming);
    }

    #[test]
    fn normalize_unknown_falls_back_to_other() {
        assert_eq!(normalize_subject(""), SubjectKey::Other);
        assert_eq!(normalize_subject("不确定"), SubjectKey::Other);
        assert_eq!(normalize_subject("化学"), SubjectKey::Other);
        assert_eq!(normalize_subject("历史"), SubjectKey::Other);
    }

    #[test]
    fn grade_band_normalization() {
        assert_eq!(normalize_grade_band("senior"), "senior");
        assert_eq!(normalize_grade_band("高中"), "senior");
        assert_eq!(normalize_grade_band("初中"), "junior");
        assert_eq!(normalize_grade_band("莫名其妙"), "");
        assert_eq!(normalize_grade_band(""), "");
    }
}
