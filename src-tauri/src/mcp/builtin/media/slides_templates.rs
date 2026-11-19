//! 演示文稿场景模板:组会 / 进度 / 答辩 / 工作汇报 / 数据分析。
//!
//! 模板只提供「推荐页序 + 主题 + 数据要求」,不做渲染;`slides_export` 据此:
//! 1) 未显式传 theme 时采用模板主题;
//! 2) 出包后在回执里给出结构与数据缺口建议(不阻断、不改写用户内容)。

use super::slides::{Layout, Slide};

pub(super) struct SlideTemplate {
    pub id: &'static str,
    pub name: &'static str,
    /// 默认主题(用户显式主题优先)
    pub theme: &'static str,
    /// 推荐最少页数
    pub min_pages: usize,
    pub need_chart: bool,
    pub need_table: bool,
    pub need_kpi: bool,
    /// 推荐页序(回执展示)
    pub plan: &'static str,
}

const TEMPLATES: &[SlideTemplate] = &[
    SlideTemplate {
        id: "group-meeting",
        name: "组会汇报",
        theme: "academic",
        min_pages: 8,
        need_chart: true,
        need_table: false,
        need_kpi: true,
        plan: "封面 → 本期概览(KPI) → 已完成 → 结果图表 → 问题与阻塞 → 下周计划 → 讨论",
    },
    SlideTemplate {
        id: "progress",
        name: "进度汇报",
        theme: "corporate",
        min_pages: 8,
        need_chart: false,
        need_table: true,
        need_kpi: true,
        plan: "封面 → 目标与范围 → 里程碑表 → 完成度(KPI) → 风险与应对 → 资源需求 → 下一步",
    },
    SlideTemplate {
        id: "defense",
        name: "答辩",
        theme: "academic",
        min_pages: 12,
        need_chart: true,
        need_table: true,
        need_kpi: false,
        plan: "封面 → 背景 → 问题定义 → 方法(图) → 实验设置(表) → 结果(图) → 对比/消融 → 结论 → 展望 → 致谢 Q&A",
    },
    SlideTemplate {
        id: "work-summary",
        name: "工作汇报总结",
        theme: "corporate",
        min_pages: 9,
        need_chart: true,
        need_table: true,
        need_kpi: true,
        plan: "封面 → 关键指标(KPI) → 业务回顾(图) → 重点工作(表) → 问题与改进 → 下阶段规划 → 结束",
    },
    SlideTemplate {
        id: "data-analysis",
        name: "数据分析",
        theme: "modern",
        min_pages: 9,
        need_chart: true,
        need_table: true,
        need_kpi: true,
        plan: "封面 → 结论先行(KPI) → 数据概览(表) → 维度拆解(图) → 归因 → 建议 → 附录口径",
    },
];

pub(super) fn lookup(raw: &str) -> Option<&'static SlideTemplate> {
    let key = raw.trim().to_ascii_lowercase();
    let canonical = match key.as_str() {
        "组会" | "组会汇报" | "group" | "weekly" => "group-meeting",
        "进度" | "进度汇报" | "milestone" => "progress",
        "答辩" | "thesis-defense" | "defence" => "defense",
        "总结" | "工作汇报" | "汇报" | "summary" => "work-summary",
        "数据分析" | "analysis" | "data" => "data-analysis",
        _ => key.as_str(),
    };
    TEMPLATES.iter().find(|t| t.id == canonical)
}

impl SlideTemplate {
    /// 结构/数据缺口提示;空表示符合模板要求。
    pub(super) fn warnings(&self, slides: &[Slide]) -> Vec<String> {
        let mut out = Vec::new();
        if slides.len() < self.min_pages {
            out.push(format!(
                "页数 {} 偏少,{}模板建议至少 {} 页",
                slides.len(),
                self.name,
                self.min_pages
            ));
        }
        let has_chart = slides.iter().any(|s| s.chart.is_some());
        let has_table = slides.iter().any(|s| s.table.is_some());
        let has_kpi = slides.iter().any(|s| !s.kpis.is_empty());
        if self.need_chart && !has_chart {
            out.push("缺少图表页:结果/趋势用 chart 呈现,比纯文字更有说服力".into());
        }
        if self.need_table && !has_table {
            out.push("缺少表格页:里程碑/对比数据建议用 table 版式".into());
        }
        if self.need_kpi && !has_kpi {
            out.push("缺少指标卡:开头用 kpi 放 2-4 个关键数字".into());
        }
        let has_closing = slides
            .iter()
            .any(|s| matches!(s.layout, Layout::Closing | Layout::Section));
        if matches!(self.id, "defense" | "group-meeting") && !has_closing {
            out.push("建议结尾加 closing 页(讨论 / Q&A)".into());
        }
        out
    }

    /// 回执中的结构建议。
    pub(super) fn plan_hint(&self) -> String {
        format!("{}模板建议页序:{}", self.name, self.plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::builtin::media::slides::{Kpi, SlideChart};

    fn slide(layout: Layout) -> Slide {
        Slide {
            title: "页".into(),
            bullets: vec![],
            image: None,
            notes: None,
            layout,
            table: None,
            kpis: vec![],
            chart: None,
        }
    }

    #[test]
    fn lookup_supports_chinese_aliases() {
        assert_eq!(lookup("组会").unwrap().id, "group-meeting");
        assert_eq!(lookup("答辩").unwrap().theme, "academic");
        assert_eq!(lookup("数据分析").unwrap().id, "data-analysis");
        assert!(lookup("unknown").is_none());
    }

    #[test]
    fn warns_on_missing_chart_and_pages() {
        let t = lookup("defense").unwrap();
        let warns = t.warnings(&[slide(Layout::Bullets)]);
        assert!(warns.iter().any(|w| w.contains("页数")));
        assert!(warns.iter().any(|w| w.contains("图表")));
        assert!(warns.iter().any(|w| w.contains("表格")));
    }

    #[test]
    fn no_warning_when_template_satisfied() {
        let t = lookup("group-meeting").unwrap();
        let mut s = slide(Layout::Chart);
        s.chart = Some(SlideChart {
            kind: "bar".into(),
            title: String::new(),
            categories: vec!["a".into()],
            series: vec![("s".into(), vec![1.0])],
        });
        s.kpis = vec![Kpi {
            value: "1".into(),
            label: "指标".into(),
        }];
        let mut closing = slide(Layout::Closing);
        closing.title = "讨论".into();
        let mut slides = vec![s, closing];
        while slides.len() < t.min_pages {
            slides.insert(slides.len() - 1, slide(Layout::Bullets));
        }
        assert!(t.warnings(&slides).is_empty());
    }
}
