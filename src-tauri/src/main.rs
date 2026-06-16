#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod state;

use commands::*;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    let app_state = state::AppState::new();
    let mut builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init());

    // WebDriver automation — debug builds only (crate is cfg(debug_assertions)-gated)
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_webdriver_automation::init());
    }

    let app = builder
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // Window
            window_minimize,
            window_maximize,
            window_close,
            window_is_maximized,
            window_platform,
            // File system
            fs_read_file,
            fs_list_dir,
            fs_write_file,
            fs_exists,
            fs_read_file_as_base64,
            fs_show_open_dialog,
            fs_show_image_dialog,
            fs_search_files,
            // Store
            store_get,
            store_set,
            store_has,
            store_delete,
            test_llm_api_key,
            // Sessions
            session_create,
            session_get,
            session_list,
            session_delete,
            session_update,
            session_add_message,
            // PTY
            pty_spawn,
            pty_write,
            pty_kill,
            pty_resize,
            pty_get_history,
            pty_has_session,
            pty_is_ready,
            pty_get_cwd,
            pty_spawn_agent,
            pty_default_shell,
            pty_foreground_process,
            pty_agent_info,
            // Athena / LLM
            athena_chat,
            athena_chat_with_session,
            athena_chat_with_images,
            summarize_agent_title,
            athena_clear_context,
            athena_set_session_context,
            athena_user_answer,
            // Output buffer / capture
            output_buffer_append,
            output_buffer_get,
            output_buffer_list,
            output_buffer_clear,
            output_capture_read,
            output_capture_list_agents,
            output_capture_get_info,
            output_capture_clear,
            get_pane_history,
            // Notifications
            notification_push,
            notification_history,
            notification_count,
            notification_mark_read,
            notification_mark_all_read,
            notification_dismiss,
            notification_clear_all,
            notification_counts,
            // Plans
            plan_create,
            plan_get,
            plan_update_step,
            // Agent comms (legacy)
            agent_comms_token,
            agent_comms_sessions,
            agent_comms_send,
            // Agents (new)
            agents_list,
            agent_get_status,
            agent_respond_input,
            agent_cancel_input,
            agent_send_message,
            agent_disconnect,
            agent_get_token,
            // Search
            search_code,
            search_ripgrep,
            // MCP
            mcp_init,
            mcp_shutdown,
            mcp_handle_request,
            mcp_broadcast,
            mcp_tools,
            // Swarm
            swarm_read_state,
            swarm_send_message,
            swarm_read_mailbox,
            // Shell integration
            shell_integration_parse,
            shell_integration_script,
            shell_integration_compatible,
            shell_integration_strip,
            // Tools
            tool_execute,
            tool_list,
            tool_openai_schema,
            // Browser
            browser_show,
            browser_hide,
            browser_navigate,
            browser_back,
            browser_forward,
            browser_reload,
            browser_open_external,
            // Plugins
            plugin_list,
            plugin_get,
            plugin_register,
            plugin_unregister,
            plugin_enable,
            plugin_disable,
            plugin_get_config,
            plugin_set_config,
            plugin_set_error,
            plugin_host_list_sessions,
            plugin_host_get_session,
            plugin_host_emit_event,
            plugin_host_subscribe,
            plugin_host_update_status,
            plugin_host_unregister_session,
            plugin_host_discover_plugins,
            plugin_host_setup_plugin,
            plugin_host_remove_plugin,
            // Security
            store_api_key,
            clear_api_key,
        ])
        .setup(|_app| Ok(()))
        .build(tauri::generate_context!("tauri.conf.json"))
        .expect("error while building tauri application");

    {
        let state = app.state::<state::AppState>();
        state.set_app_handle(app.handle().clone());
        state.wire_pty_events();
    }

    app.run(|app_handle, event| {
        match event {
            tauri::RunEvent::Ready => {
                // Real-time file persistence — no load needed
            }
            tauri::RunEvent::ExitRequested { api: _, .. } => {
                log::info!("Exit requested -- initiating graceful shutdown");

                let state = app_handle.state::<state::AppState>();

                // Shut down MCP server (synchronous — no tokio runtime on main thread)
                {
                    let mcp_server = Arc::clone(&state.mcp_server);
                    if let Ok(mut server) = mcp_server.try_lock() {
                        server.shutdown();
                    };
                }

                // Shut down agent comms
                let _ = state.agent_comms.shutdown_agent_comms();

                // Gracefully interrupt + reap every live PTY so foreground
                // processes (claude, codex, …) are closed cleanly rather than
                // orphaned. Runs before the store flush so any resume id the
                // scanner captured from the live PTY stream is already on
                // disk before we tear the runtime down.
                {
                    let sm = Arc::clone(&state.session_manager);
                    if let Ok(rt) = tokio::runtime::Handle::try_current() {
                        if let Ok(manager) = sm.try_lock() {
                            rt.block_on(manager.shutdown_all());
                        } else {
                            log::warn!("session_manager lock contended during exit; PTYs may be orphaned");
                        }
                    }
                }

                // Flush any dirty writes to disk before exit
                {
                    let store = Arc::clone(&state.store);
                    if let Ok(rt) = tokio::runtime::Handle::try_current() {
                        if let Err(e) = rt.block_on(store.flush_if_dirty()) {
                            log::error!("Failed to flush KV store on exit: {}", e);
                        }
                    }
                }

                log::info!("Graceful shutdown complete");
            }
            _ => {}
        }
    });
}
