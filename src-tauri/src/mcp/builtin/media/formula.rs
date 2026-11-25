//! 公式文本规范:把简单 LaTeX 转成可读 Unicode,原始 TeX 保留到讲稿备注。
//!
//! 支持上下标、`\frac`、`\sqrt`、希腊字母与常见符号;矩阵/多行等复杂公式
//! 不尝试排版,明确标记「当前为降级文本」,不静默丢失。

use super::formula_symbols::symbol;

/// 一段文本的转换结果。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Converted {
    pub text: String,
    /// 原始 TeX 片段(逐字保留)
    pub raw_tex: Vec<String>,
    /// 复杂公式(矩阵/多行)标记
    pub degraded: bool,
}

impl Converted {
    /// 写进讲稿备注的公式说明;无公式时返回 None。
    pub(super) fn note(&self) -> Option<String> {
        if self.raw_tex.is_empty() {
            return None;
        }
        let mut out = String::from("公式原文(TeX):");
        out.push_str(&self.raw_tex.join(" ; "));
        if self.degraded {
            out.push_str("\n公式说明:复杂公式(矩阵/多行)当前为降级文本");
        }
        Some(out)
    }
}

/// 转换整段文本:优先处理 `$...$` / `$$...$$`,否则识别整段数学命令。
pub(super) fn convert(input: &str) -> Converted {
    let mut raw = Vec::new();
    let mut degraded = false;
    if input.contains('$') {
        let mut out = String::new();
        let mut rest = input;
        while let Some(pos) = rest.find('$') {
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            let (delim, body_start) = if after.starts_with("$$") {
                ("$$", 2)
            } else {
                ("$", 1)
            };
            let body = &after[body_start..];
            let Some(end) = body.find(delim) else {
                // 未闭合的 $:原样保留,不做猜测
                out.push_str(after);
                rest = "";
                break;
            };
            let tex = &body[..end];
            let (text, deg) = convert_math(tex);
            degraded |= deg;
            raw.push(format!("{delim}{tex}{delim}"));
            out.push_str(&text);
            rest = &body[end + delim.len()..];
        }
        out.push_str(rest);
        return Converted {
            text: out,
            raw_tex: raw,
            degraded,
        };
    }
    if looks_like_tex(input) {
        let (text, deg) = convert_math(input);
        raw.push(input.to_string());
        return Converted {
            text,
            raw_tex: raw,
            degraded: deg,
        };
    }
    Converted {
        text: input.to_string(),
        raw_tex: Vec::new(),
        degraded: false,
    }
}

/// 适配 [`slides_parse`] 的调用约定:返回 (转换后文本, 是否含降级公式),
/// 并把原始 TeX 片段收集进 `raw_tex_out`(逐字保留到讲稿备注)。
pub(super) fn convert_text(input: &str, raw_tex_out: &mut Vec<String>) -> (String, bool) {
    let c = convert(input);
    raw_tex_out.extend(c.raw_tex);
    (c.text, c.degraded)
}

fn looks_like_tex(s: &str) -> bool {
    [
        "\\frac", "\\sqrt", "\\alpha", "\\beta", "\\sum", "\\int", "^{", "_{",
    ]
    .iter()
    .any(|k| s.contains(k))
}

/// 转换数学片段;返回 (可读文本, 是否复杂公式)。
fn convert_math(src: &str) -> (String, bool) {
    let degraded = src.contains("\\begin{") || src.contains("\\\\");
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let (name, next) = read_command(&chars, i + 1);
                i = next;
                let (repl, used) = apply_command(&name, &chars, i);
                i = used;
                out.push_str(&repl);
            }
            '^' | '_' => {
                let (arg, next) = read_group(&chars, i + 1);
                i = next;
                let (inner, _) = convert_math(&arg);
                let mapped = if c == '^' {
                    to_super(&inner)
                } else {
                    to_sub(&inner)
                };
                match mapped {
                    Some(s) => out.push_str(&s),
                    None => {
                        out.push(if c == '^' { '^' } else { '_' });
                        out.push_str(&format!("({inner})"));
                    }
                }
            }
            '{' | '}' => i += 1,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    let mut text = out.trim().to_string();
    if degraded {
        text.push_str("(当前为降级文本)");
    }
    (text, degraded)
}

fn read_command(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut name = String::new();
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
        name.push(chars[i]);
        i += 1;
    }
    if name.is_empty() && i < chars.len() {
        name.push(chars[i]);
        i += 1;
    }
    (name, i)
}

/// 解析 `{...}` 或单个字符;返回 (内容, 新下标)。
fn read_group(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() {
        return (String::new(), i);
    }
    if chars[i] != '{' {
        return (chars[i].to_string(), i + 1);
    }
    let mut depth = 0usize;
    let mut out = String::new();
    while i < chars.len() {
        match chars[i] {
            '{' => {
                depth += 1;
                if depth > 1 {
                    out.push('{');
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (out, i + 1);
                }
                out.push('}');
            }
            c => out.push(c),
        }
        i += 1;
    }
    (out, i)
}

fn apply_command(name: &str, chars: &[char], i: usize) -> (String, usize) {
    match name {
        "frac" => {
            let (a, n1) = read_group(chars, i);
            let (b, n2) = read_group(chars, n1);
            let (a, _) = convert_math(&a);
            let (b, _) = convert_math(&b);
            (format!("({a})/({b})"), n2)
        }
        "sqrt" => {
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '[' {
                let mut k = j + 1;
                let mut n = String::new();
                while k < chars.len() && chars[k] != ']' {
                    n.push(chars[k]);
                    k += 1;
                }
                let (x, n2) = read_group(chars, k.saturating_add(1));
                let (x, _) = convert_math(&x);
                return (format!("{n}√({x})"), n2);
            }
            let (x, n2) = read_group(chars, i);
            let (x, _) = convert_math(&x);
            (format!("√({x})"), n2)
        }
        "text" | "mathrm" | "mathbf" | "mathit" | "operatorname" => {
            let (x, n) = read_group(chars, i);
            (x, n)
        }
        "begin" | "end" => {
            let (_, n) = read_group(chars, i);
            (String::new(), n)
        }
        other => match symbol(other) {
            Some(s) => (s.to_string(), i),
            None => (format!("\\{other}"), i),
        },
    }
}

fn to_super(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.chars() {
        out.push(match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'a' => 'ᵃ',
            'b' => 'ᵇ',
            'c' => 'ᶜ',
            'd' => 'ᵈ',
            'e' => 'ᵉ',
            'f' => 'ᶠ',
            'g' => 'ᵍ',
            'h' => 'ʰ',
            'i' => 'ⁱ',
            'j' => 'ʲ',
            'k' => 'ᵏ',
            'l' => 'ˡ',
            'm' => 'ᵐ',
            'n' => 'ⁿ',
            'o' => 'ᵒ',
            'p' => 'ᵖ',
            'r' => 'ʳ',
            's' => 'ˢ',
            't' => 'ᵗ',
            'u' => 'ᵘ',
            'v' => 'ᵛ',
            'w' => 'ʷ',
            'x' => 'ˣ',
            'y' => 'ʸ',
            'z' => 'ᶻ',
            _ => return None,
        });
    }
    Some(out)
}

fn to_sub(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.chars() {
        out.push(match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            'a' => 'ₐ',
            'e' => 'ₑ',
            'h' => 'ₕ',
            'i' => 'ᵢ',
            'j' => 'ⱼ',
            'k' => 'ₖ',
            'l' => 'ₗ',
            'm' => 'ₘ',
            'n' => 'ₙ',
            'o' => 'ₒ',
            'p' => 'ₚ',
            'r' => 'ᵣ',
            's' => 'ₛ',
            't' => 'ₜ',
            'u' => 'ᵤ',
            'v' => 'ᵥ',
            'x' => 'ₓ',
            _ => return None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_latex() {
        let c = convert("能量 $E=mc^2$,系数 $a_i$,比例 $\\frac{a}{b}$,根号 $\\sqrt{x}$");
        assert!(c.text.contains("E=mc²"), "实际:{}", c.text);
        assert!(c.text.contains("aᵢ"), "实际:{}", c.text);
        assert!(c.text.contains("(a)/(b)"), "实际:{}", c.text);
        assert!(c.text.contains("√(x)"), "实际:{}", c.text);
        assert!(c.note().unwrap().contains("公式原文(TeX)"));
    }

    #[test]
    fn marks_complex_formula_as_degraded() {
        let c = convert("$$\\begin{matrix}a & b \\\\ c & d\\end{matrix}$$");
        assert!(c.degraded);
        assert!(c.text.contains("当前为降级文本"), "实际:{}", c.text);
        assert!(c.note().unwrap().contains("降级文本"));
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let c = convert("营收同比 +12%");
        assert_eq!(c.text, "营收同比 +12%");
        assert!(c.raw_tex.is_empty());
        assert!(c.note().is_none());
    }
}
