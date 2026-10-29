//! 文档模板(config)层:按用途选择排版与写作规范。
//!
//! 调研依据(GitHub 高星项目):
//! - ruanyf/document-style-guide(中文技术文档写作规范):标题层级、段落单一主题、
//!   数字半角、专有名词大小写;
//! - sparanoid/chinese-copywriting-guidelines:中英文/数字之间加空格、全角标点、
//!   不重复标点;
//! - tuna/thuthesis 等学位论文模板:三线表、表题在上且按章编号、正文小四 1.5 倍行距;
//! - pandoc reference.docx:版式应落在 Word 样式(Heading/Body Text/Caption)上,
//!   而不是逐段直接格式化,这样导航窗格、目录、样式替换才可用。

/// 文档用途模板。未显式指定时按标题/正文特征自动识别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// 实验报告:目的/原理/步骤/结果/结论,三线表 + 图表编号。
    Experiment,
    /// 论文:摘要关键词、章节化、参考文献,三线表 + 目录。
    Paper,
    /// 实践/调研报告:背景/过程/发现/建议,章节化 + 表格。
    Practice,
    /// 通用文档(说明、方案、纪要):商务排版,网格表。
    General,
}

impl Profile {
    /// 解析工具参数(中英文别名均可)。
    pub fn parse(raw: &str) -> Option<Profile> {
        let s = raw.trim().to_ascii_lowercase();
        let p = match s.as_str() {
            "experiment" | "lab" | "lab_report" | "实验" | "实验报告" => Profile::Experiment,
            "paper" | "thesis" | "论文" | "学术论文" => Profile::Paper,
            "practice" | "report" | "survey" | "实践" | "实践报告" | "调研报告" => {
                Profile::Practice
            }
            "general" | "doc" | "通用" | "通用文档" | "说明" => Profile::General,
            _ => return None,
        };
        Some(p)
    }

    /// 自动识别:先看标题,再看正文前 400 字。
    pub fn detect(title: &str, body: &str) -> Profile {
        let probe: String = body.chars().take(400).collect();
        detect_in(title)
            .or_else(|| detect_in(&probe))
            .unwrap_or(Profile::General)
    }

    /// 章节数 ≥ 2 才有目录;通用文档默认不开。
    pub fn toc_default(self) -> bool {
        !matches!(self, Profile::General)
    }

    /// 是否使用学术三线表(通用文档用网格表)。
    pub fn three_line_table(self) -> bool {
        !matches!(self, Profile::General)
    }
}

fn detect_in(text: &str) -> Option<Profile> {
    const KEYS: &[(&str, Profile)] = &[
        ("实验", Profile::Experiment),
        ("上机", Profile::Experiment),
        ("仿真", Profile::Experiment),
        ("论文", Profile::Paper),
        ("摘要", Profile::Paper),
        ("关键词", Profile::Paper),
        ("参考文献", Profile::Paper),
        ("实践", Profile::Practice),
        ("实习", Profile::Practice),
        ("调研", Profile::Practice),
    ];
    KEYS.iter().find(|(k, _)| text.contains(k)).map(|(_, p)| *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aliases() {
        assert_eq!(Profile::parse("实验报告"), Some(Profile::Experiment));
        assert_eq!(Profile::parse("paper"), Some(Profile::Paper));
        assert_eq!(Profile::parse("  通用 "), Some(Profile::General));
        assert_eq!(Profile::parse("unknown"), None);
    }

    #[test]
    fn detects_by_title_then_body() {
        assert_eq!(Profile::detect("实验三 排课 CSP", ""), Profile::Experiment);
        assert_eq!(
            Profile::detect("课程作业", "摘要：本文……\n关键词：搜索"),
            Profile::Paper
        );
        assert_eq!(
            Profile::detect("周报", "本周完成若干事项"),
            Profile::General
        );
    }

    #[test]
    fn general_profile_uses_grid_table_and_no_toc() {
        assert!(!Profile::General.toc_default());
        assert!(!Profile::General.three_line_table());
        assert!(Profile::Paper.toc_default());
        assert!(Profile::Experiment.three_line_table());
    }
}
