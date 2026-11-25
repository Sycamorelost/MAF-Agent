//! 数据图表:统一 [`ChartSpec`] 校验 + charts-rs 光栅化(PNG)。
//!
//! `layout=chart` 默认仍走 ppt-rs 原生图表;只有显式 `chart_style=raster`
//! 才会调用这里的渲染,避免改变旧输出的观感。

use charts_rs::{BarChart, HeatmapChart, LineChart, PieChart, ScatterChart, Series};
use serde_json::Value;

use super::slides::clip;

/// 图表渲染方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ChartStyle {
    #[default]
    Native,
    Raster,
}

impl ChartStyle {
    pub(super) fn parse(raw: Option<&str>) -> ChartStyle {
        match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "raster" | "png" | "image" => ChartStyle::Raster,
            _ => ChartStyle::Native,
        }
    }
}

/// 受支持的数据图表类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChartKind {
    Bar,
    Line,
    Pie,
    Scatter,
    Heatmap,
}

impl ChartKind {
    fn parse(raw: &str) -> ChartKind {
        match raw.trim().to_ascii_lowercase().as_str() {
            "line" | "trend" | "area" => ChartKind::Line,
            "pie" | "doughnut" | "ring" => ChartKind::Pie,
            "scatter" | "bubble" => ChartKind::Scatter,
            "heatmap" | "heat" => ChartKind::Heatmap,
            _ => ChartKind::Bar,
        }
    }
}

/// 与 LLM 约定的图表数据结构;渲染前先整体校验,不做静默丢弃。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChartSpec {
    pub kind: ChartKind,
    pub title: String,
    pub categories: Vec<String>,
    pub series: Vec<(String, Vec<f64>)>,
    pub unit: String,
    pub source: String,
}

/// 原生图表与光栅图表共用的单位/来源脚注。
pub(super) fn footnote(unit: &str, source: &str) -> Option<String> {
    match (unit.is_empty(), source.is_empty()) {
        (true, true) => None,
        (false, true) => Some(format!("单位:{unit}")),
        (true, false) => Some(format!("来源:{source}")),
        (false, false) => Some(format!("单位:{unit} · 来源:{source}")),
    }
}

impl ChartSpec {
    pub(super) fn new(
        kind: &str,
        title: &str,
        categories: Vec<String>,
        series: Vec<(String, Vec<f64>)>,
        unit: &str,
        source: &str,
    ) -> Result<ChartSpec, String> {
        let spec = ChartSpec {
            kind: ChartKind::parse(kind),
            title: clip(title.trim(), 40),
            categories,
            series,
            unit: clip(unit.trim(), 24),
            source: clip(source.trim(), 60),
        };
        spec.validate()?;
        Ok(spec)
    }

    /// 严格解析 raster 图表入参:不截断、不丢弃,任何缺口都报错。
    pub(super) fn from_json(v: &Value) -> Result<ChartSpec, String> {
        let cats = v
            .get("categories")
            .and_then(Value::as_array)
            .ok_or("缺少 categories 数组")?;
        if cats.is_empty() {
            return Err("categories 为空".into());
        }
        if cats.len() > 12 {
            return Err(format!("categories 有 {} 个,超过上限 12", cats.len()));
        }
        let mut categories = Vec::with_capacity(cats.len());
        for (i, c) in cats.iter().enumerate() {
            let s = match c {
                Value::String(s) => s.trim().to_string(),
                Value::Number(n) => n.to_string(),
                _ => return Err(format!("categories 第 {} 项不是文本或数字", i + 1)),
            };
            if s.is_empty() {
                return Err(format!("categories 第 {} 项为空", i + 1));
            }
            categories.push(clip(&s, 24));
        }

        let raw_series = v
            .get("series")
            .and_then(Value::as_array)
            .ok_or("缺少 series 数组")?;
        if raw_series.is_empty() {
            return Err("series 为空".into());
        }
        if raw_series.len() > 3 {
            return Err(format!("series 有 {} 条,超过上限 3", raw_series.len()));
        }
        let mut series = Vec::with_capacity(raw_series.len());
        for (si, s) in raw_series.iter().enumerate() {
            let name = clip(
                s.get("name").and_then(Value::as_str).unwrap_or("").trim(),
                24,
            );
            let name = if name.is_empty() {
                format!("系列{}", si + 1)
            } else {
                name
            };
            let values = s
                .get("values")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("系列「{name}」缺少 values 数组"))?;
            if values.len() != categories.len() {
                return Err(format!(
                    "系列「{name}」数值个数 {} 与 categories 个数 {} 不一致",
                    values.len(),
                    categories.len()
                ));
            }
            let mut nums = Vec::with_capacity(values.len());
            for (vi, x) in values.iter().enumerate() {
                let n = x
                    .as_f64()
                    .ok_or_else(|| format!("系列「{name}」第 {} 个值为空或非数值", vi + 1))?;
                if !n.is_finite() {
                    return Err(format!("系列「{name}」第 {} 个值非法", vi + 1));
                }
                nums.push(n);
            }
            series.push((name, nums));
        }
        ChartSpec::new(
            v.get("type").and_then(Value::as_str).unwrap_or("bar"),
            v.get("title").and_then(Value::as_str).unwrap_or(""),
            categories,
            series,
            v.get("unit").and_then(Value::as_str).unwrap_or(""),
            v.get("source").and_then(Value::as_str).unwrap_or(""),
        )
    }

    /// 缺值、长度不一致、空数据与非法数值都给出可定位错误。
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.categories.is_empty() {
            return Err("图表缺少 categories(分类轴数据为空)".into());
        }
        if self.series.is_empty() {
            return Err("图表缺少 series(数值系列为空)".into());
        }
        for (name, values) in &self.series {
            if values.is_empty() {
                return Err(format!("系列「{name}」没有数值"));
            }
            if values.len() != self.categories.len() {
                return Err(format!(
                    "系列「{name}」数值个数 {} 与 categories 个数 {} 不一致",
                    values.len(),
                    self.categories.len()
                ));
            }
            for (i, v) in values.iter().enumerate() {
                if !v.is_finite() {
                    return Err(format!("系列「{name}」第 {} 个数值为空或非法", i + 1));
                }
            }
        }
        if self.kind == ChartKind::Pie && self.series.len() != 1 {
            return Err(format!(
                "饼图需要 1 条系列(values 与 categories 一一对应),当前为 {} 条",
                self.series.len()
            ));
        }
        Ok(())
    }

    /// 图例下方的一行注释:单位与来源。
    pub(super) fn footnote(&self) -> Option<String> {
        footnote(&self.unit, &self.source)
    }

    /// 渲染为 PNG 字节(1200×640 画布,按宽度等比缩放)。
    pub(super) fn render_png(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let svg = self.render_svg()?;
        charts_rs::svg_to_png_with_size(&svg, Some(1200), None)
            .map_err(|e| format!("图表光栅化失败:{e}"))
    }

    fn render_svg(&self) -> Result<String, String> {
        let map_err = |e: charts_rs::Error| format!("图表渲染失败:{e}");
        let series: Vec<Series> = self
            .series
            .iter()
            .map(|(name, vals)| Series::new(name.clone(), vals.iter().map(|v| *v as f32).collect()))
            .collect();
        let cats = self.categories.clone();
        let svg = match self.kind {
            ChartKind::Bar => {
                let mut c = BarChart::new(series, cats);
                c.title.text = self.title.clone();
                c.width = 1200.0;
                c.height = 640.0;
                c.svg().map_err(map_err)?
            }
            ChartKind::Line => {
                let mut c = LineChart::new(series, cats);
                c.title.text = self.title.clone();
                c.width = 1200.0;
                c.height = 640.0;
                c.svg().map_err(map_err)?
            }
            ChartKind::Pie => {
                let vals = &self.series[0].1;
                let slices: Vec<Series> = self
                    .categories
                    .iter()
                    .zip(vals.iter())
                    .map(|(name, v)| Series::new(name.clone(), vec![*v as f32]))
                    .collect();
                let mut c = PieChart::new(slices);
                c.title.text = self.title.clone();
                c.width = 1200.0;
                c.height = 640.0;
                c.svg().map_err(map_err)?
            }
            ChartKind::Scatter => {
                // categories 为 x 轴数值标签;非数值时退化为序号
                let xs: Vec<f32> = self
                    .categories
                    .iter()
                    .enumerate()
                    .map(|(i, s)| s.trim().parse::<f32>().unwrap_or(i as f32))
                    .collect();
                let points: Vec<Series> = self
                    .series
                    .iter()
                    .map(|(name, vals)| {
                        let mut flat = Vec::with_capacity(vals.len() * 2);
                        for (i, v) in vals.iter().enumerate() {
                            flat.push(xs.get(i).copied().unwrap_or(i as f32));
                            flat.push(*v as f32);
                        }
                        Series::new(name.clone(), flat)
                    })
                    .collect();
                let mut c = ScatterChart::new(points);
                c.title.text = self.title.clone();
                c.width = 1200.0;
                c.height = 640.0;
                c.svg().map_err(map_err)?
            }
            ChartKind::Heatmap => {
                let x_len = self.categories.len();
                let mut data = Vec::new();
                let mut rows = Vec::new();
                for (r, (name, vals)) in self.series.iter().enumerate() {
                    rows.push(name.clone());
                    for (c, v) in vals.iter().enumerate() {
                        data.push((r * x_len + c, *v as f32));
                    }
                }
                let mut c = HeatmapChart::new(data, self.categories.clone(), rows);
                c.title.text = self.title.clone();
                c.width = 1200.0;
                c.height = 640.0;
                c.svg().map_err(map_err)?
            }
        };
        Ok(svg)
    }
}

/// PNG 文件头校验(测试与落盘前自检共用)。
#[cfg(test)]
pub(super) fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str) -> ChartSpec {
        ChartSpec::new(
            kind,
            "季度营收",
            vec!["Q1".into(), "Q2".into(), "Q3".into()],
            vec![("营收".into(), vec![10.0, 12.0, 15.0])],
            "万元",
            "财务系统",
        )
        .unwrap()
    }

    #[test]
    fn renders_png_magic_for_all_kinds() {
        for kind in ["bar", "line", "pie", "scatter", "heatmap"] {
            let png = spec(kind).render_png().unwrap();
            assert!(is_png(&png), "{kind} 未输出 PNG 魔数");
            assert!(png.len() > 1000, "{kind} PNG 过小");
        }
    }

    #[test]
    fn rejects_empty_and_mismatched_data() {
        assert!(ChartSpec::new("bar", "", vec![], vec![], "", "").is_err());
        assert!(
            ChartSpec::new(
                "bar",
                "",
                vec!["A".into(), "B".into()],
                vec![("s".into(), vec![1.0])],
                "",
                ""
            )
            .is_err()
        );
        assert!(
            ChartSpec::new(
                "line",
                "",
                vec!["A".into()],
                vec![("s".into(), vec![f64::NAN])],
                "",
                ""
            )
            .is_err()
        );
        let bad = serde_json::json!({
            "categories": ["A", "B"],
            "series": [{ "name": "s", "values": [1] }]
        });
        assert!(ChartSpec::from_json(&bad).unwrap_err().contains("不一致"));
    }

    #[test]
    fn footnote_mentions_unit_and_source() {
        let s = spec("bar");
        assert_eq!(s.footnote().unwrap(), "单位:万元 · 来源:财务系统");
        assert_eq!(ChartStyle::parse(Some("raster")), ChartStyle::Raster);
        assert_eq!(ChartStyle::parse(None), ChartStyle::Native);
    }
}
