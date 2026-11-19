//! 公文体版式覆盖:GB/T 9704—2012 的字号、边距与固定行距。

use super::super::style::DocStyle;

/// mm → twip(1mm ≈ 56.69 twip),四舍五入,纯整数运算。
const fn mm(v: i32) -> i32 {
    (v * 5669 + 50) / 100
}

pub(super) fn apply(s: &mut DocStyle) {
    s.latin = "Times New Roman";
    s.east_asia = "仿宋_GB2312";
    s.heading_east_asia = "方正小标宋简体";
    s.body_size = 32; // 三号 16pt
    s.body_indent = 640; // 首行缩进 2 字符
    s.body_line = 580;
    s.fixed_line = Some(580); // 每页 22 行
    s.heading_sizes = [32, 32, 32, 32];
    s.heading_before = [120, 60, 60, 60];
    s.heading_after = [60, 0, 0, 0];
    s.table_size = 28;
    s.caption_size = 28;
    s.meta_size = 32;
    s.title_size = 44; // 二号 22pt
    s.margin_top = mm(37);
    s.margin_bottom = mm(35);
    s.margin_left = mm(28);
    s.margin_right = mm(26);
    s.three_line = false;
    s.header_fill = false;
    s.justify = true;
}
