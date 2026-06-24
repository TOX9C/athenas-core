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

    // WebDriver automation — debug builds only, and only when explicitly
    // requested via TAURI_WEBVIEW_AUTOMATION (set by the `tauri-wd` e2e runner).
    //
    // The plugin's `on_webview_ready` hook calls `get_webview_window(label)` and
    // panics on `None`. That returns `None` for *child* webviews (e.g. the
    // embedded browser created via `add_child`), so registering it
    // unconditionally aborts the app the moment the in-app browser opens. Gating
    // it behind the env var keeps normal `cargo tauri dev` runs crash-free while
    // preserving e2e, which launches the app with the var set.
    #[cfg(debug_assertions)]
    if std::env::var("TAURI_WEBVIEW_AUTOMATION").is_ok() {
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
            pty_set_xterm,
            pty_foreground_process,
            pty_agent_info,
            // Trusted workspace roots
            workspace_add_trusted_root,
            workspace_remove_trusted_root,
            workspace_list_trusted_roots,
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
            browser_set_bounds,
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
                            log::warn!(
                                "session_manager lock contended during exit; PTYs may be orphaned"
                            );
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
            tauri::RunEvent::Exit => {
                // On macOS, Cmd+Q fires `Exit` (NOT `ExitRequested`), so this is
                // the reliable last chance to let live agents print their
                // `<cli> --resume <id>` line and persist it before the process
                // dies. The PTYs are still alive here (on macOS no
                // `ExitRequested` ran, so `shutdown_all` has not killed them).
                capture_resume_on_exit(app_handle);
            }
            _ => {}
        }
    });
}

static RESUME_CAPTURE_DONE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Type `/exit` into every live PTY on app exit, scan each pane's output for the
/// agent's `--resume` line, and persist it into the `workspaces` store so the
/// resume banner reappears on next launch. Idempotent (runs at most once per
/// process).
///
/// The async capture runs on a DEDICATED OS thread with its own current-thread
/// runtime; the caller blocks on the thread join. This deliberately keeps the
/// work OFF the shared multi-threaded runtime's worker threads, which must stay
/// free to keep the `pty_read_loop` tasks feeding the output buffer with the
/// agents' exit output while we poll for the resume line.
fn capture_resume_on_exit(app_handle: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    if RESUME_CAPTURE_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let app_handle = app_handle.clone();
    let worker = std::thread::Builder::new()
        .name("athena-resume-capture".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("resume capture: failed to build runtime: {e}");
                    return;
                }
            };
            rt.block_on(async {
                let state = app_handle.state::<state::AppState>();
                let n = commands::capture_resume_ids_on_exit(&state, 4000).await;
                log::info!("resume capture on exit: persisted {n} resume id(s)");
            });
        });
    match worker {
        Ok(w) => {
            let _ = w.join();
        }
        Err(e) => log::error!("resume capture: failed to spawn worker thread: {e}"),
    }
}
