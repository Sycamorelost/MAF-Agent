//! 公式矢量回退:LaTeX → 无脚本、无外链的独立 SVG。
//!
//! 复用 [`super::lexer`] 的语法树,按基线坐标系做一层简化排版:
//! 文本基线为 y=0,`asc` 向上、`desc` 向下的尺度均为像素。
//! 只输出 `<text>` / `<path>` / `<rect>` 等静态图元,不引用字体文件、
//! 不执行脚本,便于离线栅格化与 Word 包内嵌入。
//!
//! 该模块是 `formula_mode=svg` 的实现基础;默认 OMML 路线完全不变。

use super::super::inline::math_to_text;
use super::lexer::{Elm, Parser};

/// 主字号(像素 @96dpi)。
const SIZE: f32 = 32.0;
/// 画布内边距。
const PAD: f32 = 6.0;

/// 渲染结果:SVG 文本与像素尺寸。基线位置写在 SVG 的 `data-baseline` 属性中。
#[derive(Debug, Clone)]
pub struct FormulaSvg {
    pub svg: String,
    pub width: u32,
    pub height: u32,
}

/// 简化排版盒:宽度、上下延伸量与已定位的 SVG 片段。
struct Plot {
    w: f32,
    asc: f32,
    desc: f32,
    body: String,
}

impl Plot {
    fn empty() -> Self {
        Plot {
            w: 0.0,
            asc: 0.0,
            desc: 0.0,
            body: String::new(),
        }
    }
}

/// 把 LaTeX 渲染为 SVG;空公式明确失败,不静默产出空图。
pub fn render(latex: &str) -> Result<FormulaSvg, String> {
    if latex.trim().is_empty() {
        return Err("公式为空,无法生成矢量图".into());
    }
    let mut p = Parser::new(latex);
    let elms = p.parse_seq(false, false);
    let plot = layout(&elms, SIZE);
    let readable = math_to_text(latex);
    if readable.trim().is_empty() || plot.w < 0.5 {
        return Err("公式没有可渲染内容,未生成矢量图".into());
    }
    let width = (plot.w + PAD * 2.0).ceil();
    let height = (plot.asc + plot.desc + PAD * 2.0).ceil();
    let baseline = PAD + plot.asc;
    // <title> 使用可读文本,兼顾无障碍与检索;不参与排版
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" \
viewBox=\"0 0 {width:.0} {height:.0}\" data-baseline=\"{baseline:.1}\" role=\"img\">\
<title>{}</title><g transform=\"translate({PAD:.1},{baseline:.1})\">{}</g></svg>",
        xml_escape(&readable),
        plot.body
    );
    Ok(FormulaSvg {
        svg,
        width: width as u32,
        height: height as u32,
    })
}

/// 同时产出离屏 PNG 兼容副本(3 倍像素,保证打印清晰)。
pub fn render_png(latex: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let f = render(latex)?;
    let w = f.width.saturating_mul(3).max(1);
    let bytes = charts_rs::svg_to_png_with_size(&f.svg, Some(w), None)
        .map_err(|e| format!("公式矢量图栅格化失败:{e}"))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("公式 PNG 无法解码:{e}"))?;
    use image::GenericImageView;
    let (pw, ph) = img.dimensions();
    Ok((bytes, pw, ph))
}

fn layout(elms: &[Elm], size: f32) -> Plot {
    let mut acc = Plot::empty();
    for e in elms {
        let plot = node(e, size);
        acc = hcat(acc, plot);
    }
    acc
}

fn node(e: &Elm, size: f32) -> Plot {
    match e {
        Elm::Run(text, upright) => run(text, *upright, size),
        Elm::Seq(v) => layout(v, size),
        Elm::Frac(a, b) => frac(a, b, size),
        Elm::Rad(deg, body) => rad(deg.as_deref(), body, size),
        Elm::Script { base, sub, sup } => script(base, sub.as_deref(), sup.as_deref(), size),
        Elm::Delim(beg, end, body) => delim(*beg, *end, body, size),
        Elm::Accent(ch, body) => accent(*ch, body, size),
    }
}

/// 水平拼接两个盒。
fn hcat(left: Plot, right: Plot) -> Plot {
    if right.w == 0.0 && right.body.is_empty() {
        return left;
    }
    if left.body.is_empty() && left.w == 0.0 {
        return right;
    }
    let body = format!("{}{}", left.body, translate(&right.body, left.w, 0.0));
    Plot {
        w: left.w + right.w,
        asc: left.asc.max(right.asc),
        desc: left.desc.max(right.desc),
        body,
    }
}

fn run(text: &str, upright: bool, size: f32) -> Plot {
    if text.is_empty() {
        return Plot::empty();
    }
    let w: f32 = text.chars().map(|c| char_w(c, size)).sum();
    let style = if upright { "normal" } else { "italic" };
    let body = format!(
        "<text x=\"0\" y=\"0\" font-family=\"Cambria Math,STIX Two Math,serif\" \
font-size=\"{size:.1}\" font-style=\"{style}\" fill=\"#111111\" xml:space=\"preserve\">{}</text>",
        xml_escape(text)
    );
    Plot {
        w,
        asc: size * 0.78,
        desc: size * 0.22,
        body,
    }
}

fn frac(num: &[Elm], den: &[Elm], size: f32) -> Plot {
    let sub = size * 0.85;
    let n = layout(num, sub);
    let d = layout(den, sub);
    let pad = size * 0.22;
    let gap = size * 0.16;
    let w = n.w.max(d.w) + pad;
    let nx = (w - n.w) / 2.0;
    let dx = (w - d.w) / 2.0;
    let ny = -(gap + n.desc);
    let dy = gap + d.asc;
    let body = format!(
        "{}{}<rect x=\"0\" y=\"-1.2\" width=\"{w:.1}\" height=\"1.6\" fill=\"#111111\"/>",
        translate(&n.body, nx, ny),
        translate(&d.body, dx, dy)
    );
    Plot {
        w,
        asc: gap + n.desc + n.asc,
        desc: gap + d.asc + d.desc,
        body,
    }
}

fn rad(deg: Option<&[Elm]>, body: &[Elm], size: f32) -> Plot {
    let b = layout(body, size);
    let rw = (size * 0.5).max(4.0);
    let top = b.asc + size * 0.12;
    let path = format!(
        "<path d=\"M0,{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1}\" \
fill=\"none\" stroke=\"#111111\" stroke-width=\"1.4\"/>",
        -b.asc * 0.15,
        rw * 0.35,
        -b.asc * 0.32,
        rw * 0.7,
        -top,
        rw,
        -top,
        rw + b.w,
        -top
    );
    let mut plot = Plot {
        w: rw + b.w,
        asc: top,
        desc: b.desc,
        body: format!("{}{}", path, translate(&b.body, rw, 0.0)),
    };
    if let Some(deg) = deg {
        let d = layout(deg, size * 0.55);
        plot.w = plot.w.max(d.w);
        plot.asc = plot.asc.max(d.asc + size * 0.35);
        plot.body = format!(
            "{}{}",
            translate(&d.body, 0.0, -top - size * 0.05),
            plot.body
        );
    }
    plot
}

fn script(base: &[Elm], sub: Option<&[Elm]>, sup: Option<&[Elm]>, size: f32) -> Plot {
    let b = layout(base, size);
    let small = size * 0.72;
    let sub = sub.map(|s| layout(s, small));
    let sup = sup.map(|s| layout(s, small));
    let kern = size * 0.06;
    let mut plot = Plot {
        w: b.w,
        asc: b.asc,
        desc: b.desc,
        body: b.body,
    };
    // 求和/积分等大型运算符:上下标居中放于符号正上/正下
    let nary = nary_char(base).is_some();
    let (sub_x, sup_x) = if nary {
        (
            (plot.w - sub.as_ref().map(|p| p.w).unwrap_or(0.0)) / 2.0,
            (plot.w - sup.as_ref().map(|p| p.w).unwrap_or(0.0)) / 2.0,
        )
    } else {
        (plot.w + kern, plot.w + kern)
    };
    if let Some(s) = sup {
        let dy = -(size * 0.42);
        plot.asc = plot.asc.max(-dy + s.asc);
        plot.w = plot.w.max(sup_x + s.w);
        plot.body = format!("{}{}", plot.body, translate(&s.body, sup_x, dy));
    }
    if let Some(s) = sub {
        let dy = size * 0.22;
        plot.desc = plot.desc.max(dy + s.desc);
        plot.w = plot.w.max(sub_x + s.w);
        plot.body = format!("{}{}", plot.body, translate(&s.body, sub_x, dy));
    }
    plot
}

fn delim(beg: Option<char>, end: Option<char>, body: &[Elm], size: f32) -> Plot {
    let b = layout(body, size);
    let mut plot = Plot::empty();
    if let Some(c) = beg {
        plot = hcat(plot, run(&c.to_string(), true, size * 1.1));
    }
    plot = hcat(plot, b);
    if let Some(c) = end {
        plot = hcat(plot, run(&c.to_string(), true, size * 1.1));
    }
    plot
}

fn accent(mark: char, body: &[Elm], size: f32) -> Plot {
    let b = layout(body, size);
    let mut plot = Plot {
        w: b.w,
        asc: b.asc + size * 0.3,
        desc: b.desc,
        body: b.body,
    };
    if mark == '¯' {
        let line = format!(
            "<rect x=\"0\" y=\"{:.1}\" width=\"{:.1}\" height=\"1.4\" fill=\"#111111\"/>",
            -b.asc - size * 0.08,
            b.w
        );
        plot.body = format!("{}{}", line, plot.body);
        return plot;
    }
    let glyph = match mark {
        '⃗' => '→',
        other => other,
    };
    let a = run(&glyph.to_string(), true, size * 0.7);
    let x = (b.w - a.w) / 2.0;
    plot.body = format!(
        "{}{}",
        translate(&a.body, x, -b.asc - size * 0.18),
        plot.body
    );
    plot
}

fn nary_char(base: &[Elm]) -> Option<char> {
    match base {
        [Elm::Run(t, _)] if t.chars().count() == 1 => {
            let c = t.chars().next()?;
            matches!(c, '∑' | '∏' | '∬' | '∭' | '∮' | '∫').then_some(c)
        }
        _ => None,
    }
}

fn char_w(c: char, size: f32) -> f32 {
    if c.is_whitespace() {
        size * 0.28
    } else if c.is_ascii_alphanumeric() {
        size * 0.56
    } else if super::super::inline::display_width(&c.to_string()) >= 2 {
        size
    } else {
        size * 0.62
    }
}

fn translate(body: &str, dx: f32, dy: f32) -> String {
    if body.is_empty() {
        return String::new();
    }
    format!(
        "<g transform=\"translate({dx:.1},{dy:.1})\">{body}</g>"
    )
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_static_svg_with_baseline() {
        let f = render(r"E = \frac{1}{2} m v^2").unwrap();
        assert!(f.svg.starts_with("<svg"));
        assert!(f.svg.contains("viewBox="));
        assert!(f.svg.contains("data-baseline="));
        assert!(f.svg.contains("<path") || f.svg.contains("<rect"));
        assert!(!f.svg.contains("<script"));
        assert!(!f.svg.contains("http://") || f.svg.contains("www.w3.org/2000/svg"));
        assert!(f.width > 0 && f.height > 0);
    }

    #[test]
    fn accessible_title_uses_readable_text() {
        let f = render(r"\frac{a+b}{2} \le \sqrt{x^2+y^2}").unwrap();
        assert!(f.svg.contains("<title>"));
        assert!(f.svg.contains("≤"));
        assert!(f.svg.contains("√"));
    }

    #[test]
    fn deterministic_output() {
        let a = render(r"\sum_{i=1}^{n} \alpha_i x_i").unwrap();
        let b = render(r"\sum_{i=1}^{n} \alpha_i x_i").unwrap();
        assert_eq!(a.svg, b.svg);
    }

    #[test]
    fn empty_formula_is_rejected() {
        assert!(render("   ").is_err());
        assert!(render(r"\quad").is_err());
    }

    #[test]
    fn raster_fallback_produces_png() {
        let (bytes, w, h) = render_png(r"a^2+b^2=c^2").unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(w > 0 && h > 0);
    }
}
