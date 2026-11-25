//! LaTeX → OMML(Office Math)最小转换入口。
//!
//! 转换结果由 [`super::math_docx`] 在打包后写入 `word/document.xml`:
//! 正文先写 `@@MATH:0/1:base64@@` 占位符,再替换为 `<m:oMath>` / `<m:oMathPara>`。
//!
//! 覆盖论文常见写法:上下标、分式、根式、求和/积分、希腊字母、常用运算符、
//! `\text`/`\mathrm`、`\left...\right`、重音、多行环境(线性降级)。
//! 未知命令按正体文字输出,保证任何输入都能落盘。
//!
//! 职责拆分:`lexer` 词法/语法、`symbols` 符号表、`omml` XML 生成。

mod diagnostics;
mod lexer;
mod omml;
mod symbols;
pub mod vector_fallback;

use base64::{Engine as _, engine::general_purpose::STANDARD};

pub use diagnostics::{MathDiagnostic, unknown_commands};
use lexer::Parser;

pub(crate) const TOKEN: &str = "@@MATH:";

/// 行内公式占位符:display=false 用 `<m:oMath>`,true 用 `<m:oMathPara>`。
pub fn token(latex: &str, display: bool) -> String {
    format!(
        "{TOKEN}{}:{}@@",
        if display { 1 } else { 0 },
        STANDARD.encode(latex.trim().as_bytes())
    )
}

/// 生成行内 / 独立公式 XML。
///
/// 兼容模式:未知命令仍按正体文字输出,同时通过 `log::warn!` 汇总诊断,不静默吞掉。
pub fn omml(latex: &str, display: bool) -> String {
    omml_with_diagnostics(latex, display).0
}

/// 生成公式 XML 并返回诊断列表(供上层展示/日志)。
pub fn omml_with_diagnostics(latex: &str, display: bool) -> (String, Vec<MathDiagnostic>) {
    let diags = unknown_commands(latex);
    for d in &diags {
        log::warn!("docgen 公式诊断: {}", d.message);
    }
    let mut body = String::new();
    let mut p = Parser::new(latex);
    let elms = p.parse_seq(false, false);
    omml::emit(&elms, &mut body);
    let xml = if display {
        format!("<m:oMathPara><m:oMath>{body}</m:oMath></m:oMathPara>")
    } else {
        format!("<m:oMath>{body}</m:oMath>")
    };
    (xml, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_scripts_and_fraction() {
        let x = omml(r"E = \frac{1}{2} m v^2", false);
        assert!(x.contains("<m:f>"));
        assert!(x.contains("<m:sSup>"));
        assert!(x.contains("E"));
    }

    #[test]
    fn converts_sum_and_greek() {
        let x = omml(r"\sum_{i=1}^{n} \alpha_i x_i", false);
        assert!(x.contains("<m:nary>"));
        assert!(x.contains("∑"));
        assert!(x.contains("α"));
        assert!(x.contains("<m:sSub>"));
    }

    #[test]
    fn converts_sqrt_and_text() {
        let x = omml(r"\sqrt{x^2+y^2} \le \text{半径}", false);
        assert!(x.contains("<m:rad>"));
        assert!(x.contains("<m:degHide"));
        assert!(x.contains("≤"));
        assert!(x.contains("半径"));
    }

    #[test]
    fn unknown_command_degrades_to_text() {
        let x = omml(r"\weirdcmd{x}", false);
        assert!(x.contains("weirdcmd"));
    }
}
