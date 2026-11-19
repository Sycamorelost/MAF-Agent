//! OMML(Office Math Markup)XML 生成:语法树 → `<m:oMath>` 子节点。

use super::lexer::Elm;

pub(super) fn emit(elms: &[Elm], out: &mut String) {
    for e in elms {
        match e {
            Elm::Run(text, upright) => {
                if text.is_empty() {
                    continue;
                }
                out.push_str("<m:r>");
                if *upright {
                    out.push_str("<m:rPr><m:sty m:val=\"p\"/></m:rPr>");
                }
                out.push_str("<m:t>");
                escape_into(text, out);
                out.push_str("</m:t></m:r>");
            }
            Elm::Seq(v) => emit(v, out),
            Elm::Frac(a, b) => {
                out.push_str("<m:f><m:num>");
                emit(a, out);
                out.push_str("</m:num><m:den>");
                emit(b, out);
                out.push_str("</m:den></m:f>");
            }
            Elm::Rad(deg, body) => {
                out.push_str("<m:rad>");
                match deg {
                    None => out.push_str("<m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/>"),
                    Some(d) => {
                        out.push_str("<m:deg>");
                        emit(d, out);
                        out.push_str("</m:deg>");
                    }
                }
                out.push_str("<m:e>");
                emit(body, out);
                out.push_str("</m:e></m:rad>");
            }
            Elm::Script { base, sub, sup } => {
                if let Some(chr) = nary_base(base) {
                    out.push_str("<m:nary><m:naryPr><m:chr m:val=\"");
                    out.push(chr);
                    out.push_str("\"/><m:limLoc m:val=\"undOvr\"/></m:naryPr><m:sub>");
                    if let Some(s) = sub {
                        emit(s, out);
                    }
                    out.push_str("</m:sub><m:sup>");
                    if let Some(s) = sup {
                        emit(s, out);
                    }
                    out.push_str("</m:sup><m:e/></m:nary>");
                    continue;
                }
                match (sub, sup) {
                    (Some(s), Some(p)) => {
                        out.push_str("<m:sSubSup><m:e>");
                        emit(base, out);
                        out.push_str("</m:e><m:sub>");
                        emit(s, out);
                        out.push_str("</m:sub><m:sup>");
                        emit(p, out);
                        out.push_str("</m:sup></m:sSubSup>");
                    }
                    (Some(s), None) => {
                        out.push_str("<m:sSub><m:e>");
                        emit(base, out);
                        out.push_str("</m:e><m:sub>");
                        emit(s, out);
                        out.push_str("</m:sub></m:sSub>");
                    }
                    (None, Some(p)) => {
                        out.push_str("<m:sSup><m:e>");
                        emit(base, out);
                        out.push_str("</m:e><m:sup>");
                        emit(p, out);
                        out.push_str("</m:sup></m:sSup>");
                    }
                    (None, None) => emit(base, out),
                }
            }
            Elm::Delim(beg, end, body) => {
                out.push_str("<m:d><m:dPr><m:begChr m:val=\"");
                if let Some(c) = beg {
                    escape_into(&c.to_string(), out);
                }
                out.push_str("\"/><m:endChr m:val=\"");
                if let Some(c) = end {
                    escape_into(&c.to_string(), out);
                }
                out.push_str("\"/></m:dPr><m:e>");
                emit(body, out);
                out.push_str("</m:e></m:d>");
            }
            Elm::Accent(ch, body) => {
                out.push_str("<m:acc><m:accPr><m:chr m:val=\"");
                out.push(*ch);
                out.push_str("\"/></m:accPr><m:e>");
                emit(body, out);
                out.push_str("</m:e></m:acc>");
            }
        }
    }
}

fn nary_base(base: &[Elm]) -> Option<char> {
    match base {
        [Elm::Run(t, false)] if t.chars().count() == 1 => {
            let c = t.chars().next()?;
            matches!(c, '∑' | '∏' | '∫' | '∬' | '∭' | '∮').then_some(c)
        }
        _ => None,
    }
}

fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}
