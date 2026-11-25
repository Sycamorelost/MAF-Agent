//! Word 域辅助:REF(交叉引用)、SEQ(题注序号)、TOC(目录)。
//!
//! docx-rs 0.4.22 没有专用 Field 类型,但暴露了
//! `Run::add_field_char` 与 `InstrText::Unsupported`,足以拼出标准复合域:
//! `begin → instrText → separate → 显示文本 → end`。
//! 目录仍走 `TableOfContents`(行为保持不变),本模块只提供指令字符串。

use docx_rs::{FieldCharType, InstrText, Paragraph, Run};

/// 复合域的指令文本(含首尾空格,Word 要求)。
#[allow(dead_code)] // 供后续把静态引用文本升级为可刷新 REF 域
pub fn ref_instr(bookmark: &str) -> String {
    format!(" REF {bookmark} \\h ")
}

/// 题注序号域:同一 `identifier` 的 SEQ 域按出现顺序自动递增。
pub fn seq_instr(identifier: &str) -> String {
    format!(" SEQ {identifier} \\* ARABIC ")
}

/// 目录域指令(与现有 `TableOfContents` 默认行为一致)。
#[allow(dead_code)]
pub fn toc_instr(from: usize, to: usize) -> String {
    format!(" TOC \\o \"{from}-{to}\" \\h \\z \\u ")
}

/// 把「指令 + 显示文本」写成一段复合域追加到段落。
pub fn push_field(mut p: Paragraph, instr: &str, display: &str) -> Paragraph {
    p = p.add_run(Run::new().add_field_char(FieldCharType::Begin, false));
    p = p.add_run(Run::new().add_instr_text(InstrText::Unsupported(instr.to_string())));
    p = p.add_run(Run::new().add_field_char(FieldCharType::Separate, false));
    p = p.add_run(Run::new().add_text(display));
    p = p.add_run(Run::new().add_field_char(FieldCharType::End, false));
    p
}

/// 交叉引用域(REF),显示文本作为缓存结果。
#[allow(dead_code)]
pub fn push_ref(p: Paragraph, bookmark: &str, display: &str) -> Paragraph {
    push_field(p, &ref_instr(bookmark), display)
}

/// 题注序号域(SEQ),显示文本作为缓存结果。
pub fn push_seq(p: Paragraph, identifier: &str, display: &str) -> Paragraph {
    push_field(p, &seq_instr(identifier), display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instr_texts_match_word_syntax() {
        assert_eq!(ref_instr("_Ref_fig_1"), " REF _Ref_fig_1 \\h ");
        assert_eq!(seq_instr("图"), " SEQ 图 \\* ARABIC ");
        assert!(toc_instr(1, 3).contains("TOC \\o \"1-3\""));
    }

    #[test]
    fn push_field_emits_begin_instr_separate_text_end() {
        let p = push_ref(Paragraph::new(), "_Ref_tbl_2", "表 2");
        let dbg = format!("{p:?}");
        assert!(dbg.contains("Begin"), "{dbg}");
        assert!(dbg.contains("Separate"), "{dbg}");
        assert!(dbg.contains("End"), "{dbg}");
        assert!(dbg.contains("_Ref_tbl_2"), "{dbg}");
        assert!(dbg.contains("表 2"), "{dbg}");
    }
}
