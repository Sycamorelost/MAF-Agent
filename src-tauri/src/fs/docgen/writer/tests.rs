use super::super::style::CONTENT_W;
use super::table::column_widths;

fn widths_of(cells: &[&str]) -> Vec<usize> {
    let header: Vec<String> = cells.iter().map(|s| s.to_string()).collect();
    column_widths(&header, &[], cells.len())
}

#[test]
fn column_widths_fill_content_width() {
    let w = widths_of(&["模型", "推理时间", "得分"]);
    assert_eq!(w.iter().sum::<usize>(), CONTENT_W);
    assert!(w[1] > w[0], "长列应获得更大宽度: {w:?}");
    assert!(w.iter().all(|v| *v >= 720));
}

#[test]
fn column_widths_handle_single_column() {
    assert_eq!(widths_of(&["说明"]), vec![CONTENT_W]);
}
