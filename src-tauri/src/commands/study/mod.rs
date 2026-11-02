//! 学习规划相关 Tauri commands(P0 规划 + P1 学习执行)。
//!
//! 命令族:study_preflight(意图澄清)/ study_start / study_cancel / study_pause /
//! study_resume / study_list / study_get / study_export_markdown / study_delete /
//! study_deep_dig(深挖研究联动);
//! P1:study_nodes / study_unit_generate / study_quiz_new / study_quiz_submit /
//! study_mastery_set / study_node_done / study_review_queue / study_review_done / study_today。
//!
//! 职责拆分:
//! - `registry.rs`:运行中规划的控制标记与任务句柄注册表;
//! - `plan.rs`:意图澄清 + 启动/暂停/恢复/终止;
//! - `query.rs`:计划列表/详情/删除/导出/深挖;
//! - `learn_cmds.rs`:学习执行(自学材料 / 测验 / 掌握度 / 复习 / 每日任务 / 统计);
//! - `exam.rs`:学习中心(试题库 / 试卷 / 批改)。

// 命令子模块需对外可见:命令注册表使用子模块原始路径(Tauri 宏要求真实模块路径)。
pub(crate) mod exam;
pub(crate) mod learn_cmds;
pub(crate) mod plan;
pub(crate) mod query;

mod registry;

#[allow(unused_imports)]
pub use exam::{
    BankListInput, PaperAutoInput, PaperCreateInput, PaperSubmitInput, QuestionGradeInput,
    study_bank_add, study_bank_delete, study_bank_list, study_bank_update, study_paper_auto,
    study_paper_create, study_paper_delete, study_paper_get, study_paper_list, study_paper_submit,
    study_question_grade,
};
#[allow(unused_imports)]
pub use learn_cmds::{
    StudyGenInput, study_focus_log, study_mastery_set, study_memory_digest, study_node_done,
    study_nodes, study_quiz_new, study_quiz_submit, study_review_done, study_review_queue,
    study_stats, study_task_done, study_tasks_today, study_today, study_unit_generate,
};
#[allow(unused_imports)]
pub use plan::{
    StudyPreflightInput, StudyStartInput, study_cancel, study_pause, study_preflight, study_resume,
    study_start,
};
#[allow(unused_imports)]
pub use query::{
    StudyDetail, study_deep_dig, study_delete, study_export_markdown, study_get, study_list,
};
#[allow(unused_imports)]
pub use registry::{StudyCtl, StudyEntry, StudyRegistry};
