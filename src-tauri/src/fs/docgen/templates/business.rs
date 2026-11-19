//! 商务类模板覆盖:纪要沿用宋体小四,但表格用全框线。

use super::super::style::DocStyle;

pub(super) fn apply_minutes(s: &mut DocStyle) {
    s.three_line = false;
    s.header_fill = true;
}
