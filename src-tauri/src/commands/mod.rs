//! Tauri commands 集合。

pub mod agents;
pub mod attachment;
pub mod datadir;
pub mod memory;
pub mod notes;
pub mod provider;
pub mod research;
pub mod search;
pub mod settings;
pub mod study;
pub mod translate;
pub mod tutorial;

/// 汇总所有 command handler,供 Builder.invoke_handler 注册。
///
/// 用法:`tauri::Builder::default().invoke_handler(commands::all())`
macro_rules! all_commands {
    () => {
        tauri::generate_handler![
            // session
            crate::commands::memory::create_session,
            crate::commands::memory::list_sessions,
            crate::commands::memory::get_session,
            crate::commands::memory::rename_session,
            crate::commands::memory::set_session_model,
            crate::commands::memory::delete_session,
            crate::commands::memory::delete_session_if_empty,
            crate::commands::memory::prune_empty_sessions,
            crate::commands::memory::update_session_settings,
            crate::commands::memory::set_session_web_search,
            // message
            crate::commands::memory::append_message,
            crate::commands::memory::list_messages,
            crate::commands::memory::clear_messages,
            crate::commands::memory::delete_message,
            crate::commands::memory::delete_messages_from,
            crate::commands::memory::update_message,
            // 会话分支树(B6 / F4)
            crate::commands::memory::list_branches,
            crate::commands::memory::fork_branch,
            crate::commands::memory::set_active_branch,
            crate::commands::memory::rename_branch,
            crate::commands::memory::delete_branch,
            // memory.md
            crate::commands::memory::read_memory,
            crate::commands::memory::write_memory,
            crate::commands::memory::append_memory,
            crate::commands::memory::snapshot_memory,
            crate::commands::memory::list_memory_versions,
            crate::commands::memory::rollback_memory,
            crate::commands::memory::watch_memory,
            crate::commands::memory::unwatch_memory,
            crate::commands::memory::needs_summarize,
            crate::commands::memory::summarize_memory,
            // 迭代 D3
            crate::commands::memory::set_session_memory_strategy,
            crate::commands::memory::read_memory_version,
            crate::commands::memory::memory_budget_status,
            crate::commands::settings::get_settings,
            crate::commands::settings::save_settings,
            // 数据目录(可自定义)
            crate::commands::datadir::get_data_dir,
            crate::commands::datadir::pick_data_dir,
            crate::commands::datadir::set_data_dir,
            crate::commands::datadir::open_data_dir,
            crate::commands::datadir::restart_app,
            // attachment(迭代 D1 / F7 附件中心)
            crate::commands::attachment::save_attachment,
            crate::commands::attachment::read_attachment_data,
            crate::commands::attachment::list_all_attachments,
            crate::commands::attachment::insert_attachment_to_session,
            crate::commands::attachment::delete_attachment,
            // provider
            crate::commands::provider::list_provider_templates,
            crate::commands::provider::save_api_key,
            crate::commands::provider::delete_api_key,
            crate::commands::provider::list_stored_provider_ids,
            crate::commands::provider::has_api_key,
            crate::commands::provider::save_provider_template,
            crate::commands::provider::delete_provider_template,
            crate::commands::provider::restore_builtin_templates,
            crate::commands::provider::chat_stream,
            crate::commands::provider::cancel_stream,
            crate::commands::provider::generate_image,
            crate::commands::provider::list_images,
            crate::commands::provider::read_image,
            crate::commands::provider::image_to_session,
            crate::commands::provider::translate_stream,
            crate::commands::provider::fetch_provider_models,
            crate::commands::provider::list_model_cards,
            crate::commands::provider::save_model_card,
            crate::commands::provider::get_provider_model_config,
            crate::commands::provider::save_provider_model_config,
            // 思考能力(v1.9.0 工作块 I)
            crate::commands::provider::get_reasoning_capability,
            crate::commands::provider::get_reasoning_override,
            crate::commands::provider::save_reasoning_override,
            // API 流转:多 Key 槽位 + 轮换配置 + 用量统计
            crate::commands::provider::list_provider_keys,
            crate::commands::provider::save_provider_key,
            crate::commands::provider::update_key_slot,
            crate::commands::provider::delete_provider_key,
            crate::commands::provider::get_rotation_configs,
            crate::commands::provider::set_provider_rotation,
            crate::commands::provider::usage_summary,
            crate::commands::provider::usage_entries,
            crate::commands::provider::usage_key_health,
            crate::commands::provider::usage_hourly,
            // 翻译增强(迭代 D6)
            crate::commands::translate::glossary_list,
            crate::commands::translate::glossary_save,
            crate::commands::translate::save_translation,
            crate::commands::translate::list_translations,
            crate::commands::translate::delete_translation,
            // research(深度研究)
            crate::commands::research::get_search_config,
            crate::commands::research::save_search_config,
            crate::commands::research::search_test,
            crate::commands::research::research_start,
            crate::commands::research::research_cancel,
            crate::commands::research::research_pause,
            crate::commands::research::research_resume,
            crate::commands::research::research_stage_continue,
            crate::commands::research::research_update_stages,
            crate::commands::research::research_preset_list,
            crate::commands::research::research_preset_save,
            crate::commands::research::research_preset_delete,
            crate::commands::research::blueprint_list,
            crate::commands::research::blueprint_save,
            crate::commands::research::blueprint_delete,
            crate::commands::research::research_template_list,
            crate::commands::research::research_template_save,
            crate::commands::research::research_template_reset,
            crate::commands::research::research_list,
            crate::commands::research::research_get,
            crate::commands::research::research_delete,
            // 科研增强 + 报告导出(v1.9.0 工作块 II)
            crate::commands::research::research_list_skills,
            crate::commands::research::research_export_markdown,
            crate::commands::research::research_export_html,
            crate::commands::research::research_figure_data,
            crate::commands::research::pick_save_path,
            // 研究参考资料(本地文件解析与注入)
            crate::commands::research::import_research_reference,
            crate::commands::research::delete_research_reference,
            crate::commands::research::list_research_references,
            // 全局搜索
            crate::commands::search::search_all,
            // 知识库 / 笔记库(迭代 D8)
            crate::commands::notes::notes_list,
            crate::commands::notes::notes_get,
            crate::commands::notes::notes_save,
            crate::commands::notes::notes_delete,
            // Agent 模板(迭代 B2)
            crate::commands::agents::list_agent_templates,
            crate::commands::agents::save_agent_template,
            crate::commands::agents::delete_agent_template,
            crate::commands::agents::set_session_agent,
            crate::commands::agents::apply_agent_prompt,
            // 教程卡(迭代 D7.2)
            crate::commands::tutorial::list_tutorials,
            // 学习规划(板块 P0)
            crate::commands::study::study_preflight,
            crate::commands::study::study_start,
            crate::commands::study::study_cancel,
            crate::commands::study::study_pause,
            crate::commands::study::study_resume,
            crate::commands::study::study_list,
            crate::commands::study::study_get,
            crate::commands::study::study_delete,
            crate::commands::study::study_deep_dig,
            crate::commands::study::study_export_markdown,
            crate::commands::study::study_nodes,
            crate::commands::study::study_unit_generate,
            crate::commands::study::study_quiz_new,
            crate::commands::study::study_quiz_submit,
            crate::commands::study::study_mastery_set,
            crate::commands::study::study_node_done,
            crate::commands::study::study_review_queue,
            crate::commands::study::study_review_done,
            crate::commands::study::study_today,
            crate::commands::study::study_focus_log,
            crate::commands::study::study_stats,
            crate::commands::study::study_memory_digest,
        ]
    };
}

pub(crate) use all_commands;
