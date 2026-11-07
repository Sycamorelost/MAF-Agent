//! Tauri commands 集合。

pub mod agents;
pub mod attachment;
pub mod attachment_center;
pub mod attachment_quota;
pub mod attachment_rejects;
pub mod capability;
pub mod datadir;
pub mod deliverable;
pub mod diagnostics;
pub mod doc;
pub mod export;
pub mod fs;
pub mod gui;
pub mod mcp;
pub mod mcp_diagnosis;
pub mod memory;
pub mod notes;
pub mod provider;
pub mod projects;
pub mod presets;
pub mod search;
pub mod settings;
pub mod tasks;
pub mod tutorial;
pub mod websearch;

/// 汇总所有 command handler,供 Builder.invoke_handler 注册。
///
/// 用法:`tauri::Builder::default().invoke_handler(commands::all())`
macro_rules! all_commands {
    () => {
        tauri::generate_handler![
            // session
            crate::commands::memory::crud::create_session,
            crate::commands::memory::crud::list_sessions,
            crate::commands::memory::crud::get_session,
            crate::commands::memory::crud::session_get_todos,
            crate::commands::memory::crud::rename_session,
            crate::commands::memory::crud::clone_session,
            crate::commands::memory::crud::session_export_file,
            crate::commands::memory::crud::set_session_model,
            crate::commands::memory::crud::delete_session,
            crate::commands::memory::crud::delete_session_if_empty,
            crate::commands::memory::crud::prune_empty_sessions,
            crate::commands::memory::crud::update_session_settings,
            crate::commands::memory::crud::set_session_web_search,
            crate::commands::memory::crud::set_session_workbench_mode,
            crate::commands::memory::crud::set_session_permission_mode,
            // message
            crate::commands::memory::crud::append_message,
            crate::commands::memory::crud::list_messages,
            crate::commands::memory::crud::list_messages_page,
            crate::commands::memory::crud::clear_messages,
            crate::commands::memory::crud::delete_message,
            crate::commands::memory::crud::delete_messages_from,
            crate::commands::memory::crud::update_message,
            // 产物(交付卡)
            crate::commands::deliverable::list_deliverables,
            // 会话分支树(B6 / F4)
            crate::commands::memory::crud::list_branches,
            crate::commands::memory::crud::fork_branch,
            crate::commands::memory::crud::set_active_branch,
            crate::commands::memory::crud::rename_branch,
            crate::commands::memory::crud::delete_branch,
            // memory.md
            crate::commands::memory::diff::read_memory,
            crate::commands::memory::diff::write_memory,
            crate::commands::memory::diff::append_memory,
            crate::commands::memory::diff::snapshot_memory,
            crate::commands::memory::diff::list_memory_versions,
            crate::commands::memory::diff::rollback_memory,
            crate::commands::memory::diff::watch_memory,
            crate::commands::memory::diff::unwatch_memory,
            crate::commands::memory::diff::needs_summarize,
            crate::commands::memory::diff::summarize_memory,
            // 迭代 D3
            crate::commands::memory::crud::set_session_memory_strategy,
            crate::commands::memory::diff::read_memory_version,
            crate::commands::memory::diff::memory_budget_status,
            crate::commands::settings::get_settings,
            crate::commands::settings::save_settings,
            // 数据目录(可自定义)
            crate::commands::datadir::get_data_dir,
            crate::commands::datadir::pick_data_dir,
            crate::commands::datadir::set_data_dir,
            crate::commands::datadir::open_data_dir,
            crate::commands::datadir::restart_app,
            // attachment(迭代 D1 / F7 附件中心);命令注册使用子模块原始路径。
            crate::commands::attachment::save::save_attachment,
            crate::commands::attachment::save::extract_text_preview,
            crate::commands::attachment::save::read_attachment_data,
            crate::commands::attachment::center::list_all_attachments,
            crate::commands::attachment::center::insert_attachment_to_session,
            crate::commands::attachment::center::delete_attachment,
            // 附件中心增强(F7.1):批量清理 / 失败重试 / 拒绝原因留存
            crate::commands::attachment_center::delete_attachments_batch,
            crate::commands::attachment_center::retry_attachment_extract,
            crate::commands::attachment_rejects::record_attachment_rejects,
            crate::commands::attachment_rejects::list_attachment_rejects,
            crate::commands::attachment_rejects::clear_attachment_rejects,
            // 文档预检索(R4:发送前本地检索,不依赖模型主动调工具)
            crate::commands::doc::doc_prefetch,
            // provider
            crate::commands::provider::templates::list_provider_templates,
            crate::commands::provider::keys::save_api_key,
            crate::commands::provider::keys::delete_api_key,
            crate::commands::provider::keys::list_stored_provider_ids,
            crate::commands::provider::keys::has_api_key,
            crate::commands::provider::templates::save_provider_template,
            crate::commands::provider::templates::set_provider_models,
            crate::commands::provider::templates::delete_provider_template,
            crate::commands::provider::templates::restore_builtin_templates,
            crate::commands::provider::chat::command::chat_stream,
            crate::commands::provider::stream::cancel_stream,
            crate::commands::provider::image::generate_image,
            crate::commands::provider::image::list_images,
            crate::commands::provider::image::read_image,
            crate::commands::provider::image::save_image_as,
            crate::commands::provider::models::fetch_provider_models,
            crate::commands::provider::models::list_model_cards,
            crate::commands::provider::models::save_model_card,
            crate::commands::provider::models::get_provider_model_config,
            crate::commands::provider::models::save_provider_model_config,
            // 思考能力(v1.9.0 工作块 I)
            crate::commands::provider::models::get_reasoning_capability,
            crate::commands::provider::models::get_reasoning_override,
            crate::commands::provider::models::save_reasoning_override,
            // API 流转:多 Key 槽位 + 轮换配置 + 用量统计
            crate::commands::provider::keys::list_provider_keys,
            crate::commands::provider::keys::save_provider_key,
            crate::commands::provider::keys::update_key_slot,
            crate::commands::provider::keys::delete_provider_key,
            crate::commands::provider::keys::get_rotation_configs,
            crate::commands::provider::keys::set_provider_rotation,
            crate::commands::provider::usage::usage_summary,
            crate::commands::provider::usage::usage_entries,
            crate::commands::provider::usage::usage_key_health,
            crate::commands::provider::usage::usage_hourly,
            crate::commands::provider::usage::usage_recalc,
            // 项目(P8.1 项目化升级)
            crate::commands::projects::project_list,
            crate::commands::projects::project_create,
            crate::commands::projects::project_rename,
            crate::commands::projects::project_update_path,
            crate::commands::projects::project_update_model,
            crate::commands::projects::project_update_context,
            crate::commands::projects::project_delete,
            crate::commands::projects::project_update_sort,
            crate::commands::projects::session_set_project,
            // 运行时预设(P5 / D18 / D19)
            crate::commands::presets::list_presets,
            crate::commands::presets::resolve_session_preset,
            crate::commands::presets::set_session_preset,
            // 诊断:静默失败计数(P2,只读)
            crate::commands::diagnostics::diagnostics_failure_counts,
            // 联网搜索(P0 自 research 迁出,IPC 名不变)
            crate::commands::websearch::get_search_config,
            crate::commands::websearch::save_search_config,
            crate::commands::websearch::search_test,
            // 工作台任务计划(harness 目标级任务)
            crate::commands::tasks::task_create,
            crate::commands::tasks::task_list,
            crate::commands::tasks::task_update,
            crate::commands::tasks::task_set_plan,
            crate::commands::tasks::task_delete,
            // 通用导出(另存为令牌 / 打印导出 PDF)
            crate::commands::export::pick_save_path,
            crate::commands::export::export_pdf,
            crate::commands::export::export_file_pdf,
            // 全局搜索
            crate::commands::search::search_all,
            // 知识库 / 笔记库(迭代 D8)
            crate::commands::notes::notes_list,
            crate::commands::notes::notes_get,
            crate::commands::notes::notes_save,
            crate::commands::notes::notes_delete,
            crate::commands::notes::notes_reimport_presets,
            // Agent 模板(迭代 B2)
            crate::commands::agents::list_agent_templates,
            crate::commands::agents::save_agent_template,
            crate::commands::agents::delete_agent_template,
            crate::commands::agents::export_agent_template,
            crate::commands::agents::export_agent_to_file,
            crate::commands::agents::pick_agent_file,
            crate::commands::agents::import_agent_template,
            crate::commands::agents::set_session_agent,
            crate::commands::agents::apply_agent_prompt,
            // 教程卡(迭代 D7.2)
            crate::commands::tutorial::list_tutorials,
            // MCP 工具(Phase 4:配置管理 + 内置时间;Phase 5:运行时 + HITL 审批)
            crate::commands::mcp::mcp_load_config,
            crate::commands::mcp::get_bootstrap_state,
            crate::commands::mcp::mcp_save_config,
            crate::commands::memory::longterm::mcp_clear_memory,
            crate::commands::memory::longterm::mcp_memory_graph,
            crate::commands::mcp::mcp_add_server,
            crate::commands::mcp::mcp_remove_server,
            crate::commands::mcp::mcp_start_server,
            crate::commands::mcp::mcp_start_all_enabled,
            crate::commands::mcp::mcp_list_tools,
            crate::commands::mcp::mcp_call_tool_with_approval,
            crate::commands::mcp::mcp_resolve_approval,
            crate::commands::mcp::mcp_clear_session_allowed,
            crate::commands::mcp::mcp_collect_tools,
            // P4:工具调用记录全文(按 id 按需加载)
            crate::commands::mcp::mcp_get_tool_log,
            crate::commands::mcp::mcp_get_approval_policy,
            crate::commands::mcp::mcp_set_approval_policy,
            // Phase 6:白名单 shell 命令(直接 Rust,极简安全)
            crate::commands::mcp::shell_whitelist_exec,
            // C2:P0a 稳定性保底 —— mcp_doctor 诊断命令(二进制存在/版本/能否启动/失败原因)
            crate::commands::mcp_diagnosis::mcp_doctor,
            // P2:能力令牌(预授权,仅内存)
            crate::commands::capability::mcp_issue_capability,
            crate::commands::capability::mcp_list_capabilities,
            crate::commands::capability::mcp_revoke_capability,
            crate::commands::capability::mcp_clear_capabilities,
            // P2:GUI 自动化策略与自检
            crate::commands::gui::gui_get_policy,
            crate::commands::gui::gui_save_policy,
            crate::commands::gui::gui_list_windows,
            crate::commands::gui::gui_find_elements,
            // 文件服务(无沙箱 + 备份回滚 + 办公格式导出)
            crate::commands::fs::fs_workspace_info,
            crate::commands::fs::fs_list,
            crate::commands::fs::fs_read_text,
            crate::commands::fs::fs_write_text,
            crate::commands::fs::fs_delete,
            crate::commands::fs::fs_backup_list,
            crate::commands::fs::fs_backup_read,
            crate::commands::fs::fs_restore,
            crate::commands::fs::fs_export_docx,
            crate::commands::fs::fs_export_xlsx,
            crate::commands::fs::fs_extract_text,
            crate::commands::fs::fs_zip_list,
            crate::commands::fs::fs_zip_extract,
            crate::commands::fs::fs_open_workspace,
            crate::commands::fs::fs_pick_workspace_dir,
            crate::commands::fs::fs_open_backups,
            crate::commands::fs::fs_open,
            crate::commands::fs::fs_reveal,
            crate::commands::fs::fs_read_data_url,
        ]
    };
}

pub(crate) use all_commands;
