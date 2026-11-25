//! 公式诊断:未知 LaTeX 命令不再静默降级为正体文字,而是产出结构化诊断。
//!
//! 只做命令级检测,不解析语法;已知命令 = 符号表 ∪ 排版函数表 ∪ 本模块白名单。

use super::symbols::{symbol, upright_fn};

/// 一条公式诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathDiagnostic {
    pub command: String,
    pub message: String,
}

/// 白名单:lexer 直接处理、但不在符号/函数表中的命令。
const KNOWN: &[&str] = &[
    "frac",
    "dfrac",
    "tfrac",
    "sqrt",
    "text",
    "textrm",
    "mbox",
    "operatorname",
    "mathrm",
    "mathbf",
    "mathit",
    "mathcal",
    "mathbb",
    "left",
    "right",
    "vec",
    "bar",
    "hat",
    "tilde",
    "dot",
    "ddot",
    "overline",
    "begin",
    "end",
    "quad",
    "qquad",
    "enspace",
    "thinspace",
    "displaystyle",
    "textstyle",
    "scriptstyle",
    "limits",
    "nolimits",
    "langle",
    "rangle",
    "lceil",
    "rceil",
    "lfloor",
    "rfloor",
];

/// 扫描 LaTeX 中的未知命令。
pub fn unknown_commands(latex: &str) -> Vec<MathDiagnostic> {
    let chars: Vec<char> = latex.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '\\' {
            i += 1;
            continue;
        }
        i += 1;
        let mut cmd = String::new();
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            cmd.push(chars[i]);
            i += 1;
        }
        if cmd.is_empty() {
            // `\,` `\\` `\{` 等单字符命令由 lexer 直接处理
            i += 1;
            continue;
        }
        if symbol(&cmd).is_some() || upright_fn(&cmd).is_some() || KNOWN.contains(&cmd.as_str()) {
            continue;
        }
        out.push(MathDiagnostic {
            command: cmd.clone(),
            message: format!("未知 LaTeX 命令 \\{cmd},已按正体文字输出"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unknown_but_accepts_known() {
        let d = unknown_commands(r"\frac{a}{b} + \alpha \weirdcmd{x}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].command, "weirdcmd");
    }

    #[test]
    fn ignores_escaped_single_char_commands() {
        assert!(unknown_commands(r"a \{b\} \, c \\ d").is_empty());
    }
}
