//! 行内处理层:Markdown 行内标记 → 片段;中文排版规范化;LaTeX 兜底。
//!
//! 规则来源:
//! - sparanoid/chinese-copywriting-guidelines:中英文/数字之间加空格、不重复使用标点;
//! - ruanyf/document-style-guide:数字用半角、段落陈述语气;
//! - 实验报告/论文常见问题:模型输出的 `$X=\{M_1,\dots\}$` 直接落进 Word 可读性极差,
//!   这里做保守的可读化转换(不做完整公式排版,只去掉标记并换成 Unicode 符号)。

/// 行内样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Normal,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Strike,
}

/// 一段同一样式的文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub emphasis: Emphasis,
}

impl Segment {
    fn new(text: String, emphasis: Emphasis) -> Self {
        Segment { text, emphasis }
    }
}

/// 解析行内 Markdown(粗体/斜体/删除线/行内代码/链接/数学公式)。
pub fn parse_inline(text: &str) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut buf = String::new();
    let (mut bold, mut italic, mut strike) = (false, false, false);
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '`' => {
                let n = run_len(&chars, i, '`');
                if let Some(end) = find_run(&chars, i + n, '`', n) {
                    flush(&mut out, &mut buf, bold, italic, strike);
                    let code: String = chars[i + n..end].iter().collect();
                    out.push(Segment::new(code.trim().to_string(), Emphasis::Code));
                    i = end + n;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '\\' => {
                if i + 1 < chars.len() {
                    buf.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '*' | '_' => {
                let is_star = c == '*';
                let double = i + 1 < chars.len() && chars[i + 1] == c;
                if double {
                    // `~~` 只在 `~` 上成立,星号/下划线双写为粗体
                    flush(&mut out, &mut buf, bold, italic, strike);
                    bold = !bold;
                    i += 2;
                } else if is_star {
                    flush(&mut out, &mut buf, bold, italic, strike);
                    italic = !italic;
                    i += 1;
                } else {
                    // 下划线单写不作斜体,避免误伤 snake_case 标识符
                    buf.push(c);
                    i += 1;
                }
            }
            '~' if i + 1 < chars.len() && chars[i + 1] == '~' => {
                flush(&mut out, &mut buf, bold, italic, strike);
                strike = !strike;
                i += 2;
            }
            '$' => {
                if let Some(end) = find_dollar(&chars, i + 1) {
                    flush(&mut out, &mut buf, bold, italic, strike);
                    let raw: String = chars[i + 1..end].iter().collect();
                    out.push(Segment::new(math_to_text(&raw), Emphasis::Normal));
                    i = end + 1;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '!' if i + 1 < chars.len() && chars[i + 1] == '[' => {
                if let Some((label, next)) = link_label(&chars, i + 2) {
                    // 图片在块级处理;行内出现时保留替代文字
                    buf.push_str(&label);
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '[' => {
                if let Some((label, next)) = link_label(&chars, i + 1) {
                    buf.push_str(&label);
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    flush(&mut out, &mut buf, bold, italic, strike);
    fix_boundaries(out)
}

fn run_len(chars: &[char], start: usize, ch: char) -> usize {
    let mut n = 0;
    while start + n < chars.len() && chars[start + n] == ch {
        n += 1;
    }
    n
}

fn find_run(chars: &[char], start: usize, ch: char, n: usize) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == ch && run_len(chars, i, ch) >= n {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_dollar(chars: &[char], start: usize) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == '$')
}

/// `[text](url)` / `![alt](src)` 的标签部分;返回 (标签, 结束位置)。
fn link_label(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    let mut label = String::new();
    let mut depth = 0usize;
    while i < chars.len() {
        match chars[i] {
            '[' => {
                depth += 1;
                label.push('[');
            }
            ']' => {
                if depth == 0 {
                    // 期待 `(url)`
                    if i + 1 < chars.len() && chars[i + 1] == '(' {
                        let mut j = i + 2;
                        while j < chars.len() && chars[j] != ')' {
                            j += 1;
                        }
                        if j < chars.len() {
                            return Some((label, j + 1));
                        }
                    }
                    return None;
                }
                depth -= 1;
                label.push(']');
            }
            c => label.push(c),
        }
        i += 1;
    }
    None
}

fn flush(out: &mut Vec<Segment>, buf: &mut String, bold: bool, italic: bool, strike: bool) {
    if buf.trim().is_empty() {
        buf.clear();
        return;
    }
    let emphasis = match (bold, italic, strike) {
        (true, true, _) => Emphasis::BoldItalic,
        (true, false, _) => Emphasis::Bold,
        (false, true, _) => Emphasis::Italic,
        (false, false, true) => Emphasis::Strike,
        _ => Emphasis::Normal,
    };
    out.push(Segment::new(normalize_zh(buf), emphasis));
    buf.clear();
}

/// 相邻片段之间补空格(避免粗体边界吞掉中英间距)。
fn fix_boundaries(mut segs: Vec<Segment>) -> Vec<Segment> {
    for i in 1..segs.len() {
        if segs[i].emphasis == Emphasis::Code || segs[i - 1].emphasis == Emphasis::Code {
            continue;
        }
        let prev = segs[i - 1].text.chars().last();
        let next = segs[i].text.chars().next();
        if let (Some(a), Some(b)) = (prev, next)
            && need_space(a, b)
        {
            segs[i].text.insert(0, ' ');
            // 空的占位片段清理
            if segs[i].text.trim().is_empty() {
                segs[i].text.clear();
            }
        }
    }
    segs.retain(|s| !s.text.is_empty());
    segs
}

/// 中文排版规范化:中英/中数之间加空格,收敛重复的感叹/问号。
pub fn normalize_zh(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut last_punct: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            if need_space(prev, c) && !out.ends_with(' ') {
                out.push(' ');
            }
        }
        // 不重复使用标点:!!! → !,?? → ?(全角同理)
        if matches!(c, '!' | '?' | '！' | '？') {
            if last_punct == Some(c) {
                continue;
            }
            last_punct = Some(c);
        } else {
            last_punct = None;
        }
        out.push(c);
    }
    out
}

fn need_space(a: char, b: char) -> bool {
    (is_han(a) && b.is_ascii_alphanumeric()) || (a.is_ascii_alphanumeric() && is_han(b))
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

/// 两行软换行合并:西文单词之间补空格,中文直接相接。
pub fn join_soft(a: &str, b: &str) -> String {
    match (a.chars().last(), b.chars().next()) {
        (Some(x), Some(y)) if x.is_ascii_alphanumeric() && y.is_ascii_alphanumeric() => {
            format!("{a} {b}")
        }
        _ => format!("{a}{b}"),
    }
}

/// 显示宽度:汉字/全角按 2 列计,用于表格列宽估算。
pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| {
            if is_han(c) || matches!(c as u32, 0x3000..=0x303F | 0xFF01..=0xFF60) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// LaTeX 片段 → 可读文本(保守转换,未知命令仅去反斜杠)。
pub fn math_to_text(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            i += 1;
            if i >= chars.len() {
                break;
            }
            let mut cmd = String::new();
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                cmd.push(chars[i]);
                i += 1;
            }
            if cmd.is_empty() {
                let sym = chars[i];
                i += 1;
                match sym {
                    '{' => out.push('{'),
                    '}' => out.push('}'),
                    '\\' => out.push_str("; "),
                    ',' | ';' | '!' | ' ' => {}
                    other => out.push(other),
                }
                continue;
            }
            match cmd.as_str() {
                "times" => out.push('×'),
                "cdot" => out.push('·'),
                "le" | "leq" => out.push('≤'),
                "ge" | "geq" => out.push('≥'),
                "ne" | "neq" => out.push('≠'),
                "approx" => out.push('≈'),
                "pm" => out.push('±'),
                "infty" => out.push('∞'),
                "in" => out.push('∈'),
                "notin" => out.push('∉'),
                "subset" => out.push('⊂'),
                "subseteq" => out.push('⊆'),
                "cup" => out.push('∪'),
                "cap" => out.push('∩'),
                "forall" => out.push('∀'),
                "exists" => out.push('∃'),
                "to" | "rightarrow" => out.push('→'),
                "leftarrow" => out.push('←'),
                "Rightarrow" => out.push('⇒'),
                "Leftarrow" => out.push('⇐'),
                "sum" => out.push('∑'),
                "prod" => out.push('∏'),
                "sqrt" => out.push('√'),
                "dots" | "cdots" | "ldots" => out.push('…'),
                "mid" | "vert" => out.push('|'),
                "quad" | "qquad" => out.push(' '),
                // 命令名本身就是「按原样排版内容」的容器
                "text" | "mathrm" | "mathbf" | "mathit" | "operatorname" | "left" | "right" => {}
                other => out.push_str(other),
            }
            continue;
        }
        match c {
            '{' | '}' => i += 1,
            '^' | '_' => {
                let marker = c;
                i += 1;
                let mut token = String::new();
                if i < chars.len() && chars[i] == '{' {
                    let mut depth = 1usize;
                    i += 1;
                    while i < chars.len() && depth > 0 {
                        match chars[i] {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    i += 1;
                                    break;
                                }
                            }
                            other => token.push(other),
                        }
                        i += 1;
                    }
                } else if i < chars.len() {
                    token.push(chars[i]);
                    i += 1;
                }
                match script_text(marker, &token) {
                    Some(conv) => out.push_str(&conv),
                    None => {
                        out.push(marker);
                        out.push_str(&math_to_text(&token));
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out.trim().to_string()
}

/// 上下标转换:数字与少量字母转 Unicode,其余原样返回。
fn script_text(marker: char, token: &str) -> Option<String> {
    const SUP: &[(char, char)] = &[
        ('0', '⁰'),
        ('1', '¹'),
        ('2', '²'),
        ('3', '³'),
        ('4', '⁴'),
        ('5', '⁵'),
        ('6', '⁶'),
        ('7', '⁷'),
        ('8', '⁸'),
        ('9', '⁹'),
        ('+', '⁺'),
        ('-', '⁻'),
        ('=', '⁼'),
        ('(', '⁽'),
        (')', '⁾'),
        ('n', 'ⁿ'),
        ('i', 'ⁱ'),
    ];
    const SUB: &[(char, char)] = &[
        ('0', '₀'),
        ('1', '₁'),
        ('2', '₂'),
        ('3', '₃'),
        ('4', '₄'),
        ('5', '₅'),
        ('6', '₆'),
        ('7', '₇'),
        ('8', '₈'),
        ('9', '₉'),
        ('+', '₊'),
        ('-', '₋'),
        ('=', '₌'),
        ('(', '₍'),
        (')', '₎'),
        ('a', 'ₐ'),
        ('e', 'ₑ'),
        ('h', 'ₕ'),
        ('i', 'ᵢ'),
        ('j', 'ⱼ'),
        ('k', 'ₖ'),
        ('l', 'ₗ'),
        ('m', 'ₘ'),
        ('n', 'ₙ'),
        ('o', 'ₒ'),
        ('p', 'ₚ'),
        ('r', 'ᵣ'),
        ('s', 'ₛ'),
        ('t', 'ₜ'),
        ('u', 'ᵤ'),
        ('v', 'ᵥ'),
        ('x', 'ₓ'),
    ];
    let table = if marker == '^' { SUP } else { SUB };
    let mut out = String::new();
    for c in token.chars() {
        match table.iter().find(|(k, _)| *k == c) {
            Some((_, v)) => out.push(*v),
            // 含空格、逗号等复杂内容时放弃转换,退回原样
            None => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bold_and_code() {
        let segs = parse_inline("这是 **粗体** 与 `code` 示例");
        let emphases: Vec<Emphasis> = segs.iter().map(|s| s.emphasis).collect();
        assert!(emphases.contains(&Emphasis::Bold));
        assert!(emphases.contains(&Emphasis::Code));
        assert_eq!(
            segs.iter()
                .find(|s| s.emphasis == Emphasis::Code)
                .unwrap()
                .text,
            "code"
        );
    }

    #[test]
    fn adds_space_between_chinese_and_ascii() {
        assert_eq!(normalize_zh("使用Python实现"), "使用 Python 实现");
        assert_eq!(normalize_zh("共 12 个"), "共 12 个");
        // 标点两侧不加空格
        assert_eq!(normalize_zh("结束。Next"), "结束。Next");
    }

    #[test]
    fn collapses_repeated_punctuation() {
        assert_eq!(normalize_zh("注意！！！"), "注意！");
        assert_eq!(normalize_zh("真的??"), "真的?");
    }

    #[test]
    fn joins_soft_wrapped_lines() {
        assert_eq!(join_soft("hello", "world"), "hello world");
        assert_eq!(join_soft("中文", "换行"), "中文换行");
    }

    #[test]
    fn converts_latex_to_readable_text() {
        assert_eq!(math_to_text(r"X=\{M_1,\dots,M_{12}\}"), "X={M₁,…,M₁₂}");
        assert_eq!(math_to_text(r"a \times b \le c"), "a × b ≤ c");
        assert_eq!(math_to_text(r"x^2 + y^2"), "x² + y²");
        assert_eq!(math_to_text(r"\text{时间} \mid 10"), "时间 | 10");
    }

    #[test]
    fn display_width_counts_cjk_as_two() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("中文"), 4);
    }
}
