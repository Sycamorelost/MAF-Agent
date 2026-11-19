//! 标题编号:三套方案与「用户已手写编号」检测。
//!
//! 只做字符串组装,不关心样式;由 writer 在写标题前调用。

use super::super::style::HeadingNumbering;

/// 生成某一级标题的前缀。`counters` 为各一级计数(0 起不含,调用方已自增)。
pub fn prefix(kind: HeadingNumbering, counters: &[usize; 4], level: usize) -> String {
    let i = level.saturating_sub(1).min(3);
    match kind {
        HeadingNumbering::None => String::new(),
        HeadingNumbering::Academic => match i {
            0 => format!("{} ", counters[0]),
            1 => format!("{}.{} ", counters[0], counters[1]),
            2 => format!("{}.{}.{} ", counters[0], counters[1], counters[2]),
            _ => format!("({}) ", counters[3]),
        },
        HeadingNumbering::Official => match i {
            0 => format!("{}、", cn(counters[0])),
            1 => format!("（{}）", cn(counters[1])),
            2 => format!("{}.", counters[2]),
            _ => format!("（{}）", counters[3]),
        },
        HeadingNumbering::Simple => match i {
            0 => format!("{}、", cn(counters[0])),
            1 => format!("{}.", counters[1]),
            2 => format!("（{}）", counters[2]),
            _ => format!("（{}）", counters[3]),
        },
    }
}

/// 用户已手写编号(一、/（一）/1./1.1/第X章)时不自动追加。
pub fn has_manual(text: &str) -> bool {
    let t = text.trim();
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if "一二三四五六七八九十".contains(first) {
        return chars.next().is_some_and(|c| c == '、' || c == ' ' || c == '.');
    }
    if first == '（' || first == '(' {
        return true;
    }
    if first.is_ascii_digit() {
        let rest: String = t.chars().take(8).collect();
        return rest.contains('.') || rest.contains('、');
    }
    t.starts_with("第") && t.chars().count() > 2 && t.contains('章')
}

fn cn(n: usize) -> String {
    const DIGITS: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    match n {
        0 => "零".into(),
        1..=9 => DIGITS[n].into(),
        10 => "十".into(),
        11..=19 => format!("十{}", DIGITS[n % 10]),
        20..=99 if n % 10 == 0 => format!("{}十", DIGITS[n / 10]),
        20..=99 => format!("{}十{}", DIGITS[n / 10], DIGITS[n % 10]),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn academic_numbering_is_decimal_hierarchy() {
        let c = [2, 3, 4, 1];
        assert_eq!(prefix(HeadingNumbering::Academic, &c, 1), "2 ");
        assert_eq!(prefix(HeadingNumbering::Academic, &c, 2), "2.3 ");
        assert_eq!(prefix(HeadingNumbering::Academic, &c, 3), "2.3.4 ");
        assert_eq!(prefix(HeadingNumbering::Academic, &c, 4), "(1) ");
    }

    #[test]
    fn official_numbering_uses_chinese_numerals() {
        let c = [12, 3, 4, 2];
        assert_eq!(prefix(HeadingNumbering::Official, &c, 1), "十二、");
        assert_eq!(prefix(HeadingNumbering::Official, &c, 2), "（三）");
        assert_eq!(prefix(HeadingNumbering::Official, &c, 3), "4.");
        assert_eq!(prefix(HeadingNumbering::Official, &c, 4), "（2）");
    }

    #[test]
    fn detects_manual_numbering() {
        assert!(has_manual("一、实验目的"));
        assert!(has_manual("（二）方法"));
        assert!(has_manual("1.1 背景"));
        assert!(has_manual("第 3 章 实验"));
        assert!(!has_manual("实验目的"));
        assert!(!has_manual("CSP 求解"));
    }
}
