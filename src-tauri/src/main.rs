#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod relay;
mod state;
use commands::*;
use std::sync::Arc;
use tauri::Manager;

#[cfg(debug_assertions)]
fn relay_autostart_requested() -> bool {
    std::env::var("ATHENA_RELAY_AUTOSTART")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
fn relay_autostart_requested() -> bool {
    false
}

fn main() {
    let app_state = state::AppState::new();
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        // Save/restore the main window's size, position, and maximized
        // state across relaunches. Saved state overrides the window
        // config's static width/height/maximized values on launch.
        .plugin(tauri_plugin_window_state::Builder::default().build());

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
    let builder = if std::env::var("TAURI_WEBVIEW_AUTOMATION").is_ok() {
        builder.plugin(tauri_plugin_webdriver_automation::init())
    } else {
        builder
    };

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
            read_clipboard_text,
            pty_kill,
            pty_resize,
            pty_get_history,
            pty_has_session,
            pty_is_ready,
            pty_get_cwd,
            pty_spawn_agent,
            pty_default_shell,
            pty_set_xterm,
            pty_attach_listener,
            pty_detach_listener,
            pty_foreground_process,
            pty_agent_info,
            // Trusted workspace roots
            workspace_add_trusted_root,
            workspace_remove_trusted_root,
            workspace_list_trusted_roots,
            // Athena / LLM
            athena_chat,
            athena_chat_stream,
            athena_cancel_stream,
            athena_chat_with_session,
            athena_chat_with_images,
            summarize_agent_title,
            athena_clear_context,
            athena_set_session_context,
            athena_user_answer,
            llm_list_models,
            // Output buffer / capture
            output_buffer_append,
            output_buffer_get,
            output_buffer_list,
            output_buffer_clear,
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
            swarm_create,
            swarm_read_state,
            swarm_start_watch,
            swarm_stop_watch,
            swarm_update_agent,
            swarm_set_status,
            swarm_create_task,
            swarm_update_task,
            swarm_send_message,
            swarm_read_mailbox,
            // Shell integration
            shell_integration_parse,
            shell_integration_script,
            shell_integration_compatible,
            shell_integration_strip,
            // Browser
            browser_show,
            browser_hide,
            browser_navigate,
            browser_back,
            browser_forward,
            browser_reload,
            browser_set_bounds,
            // Kanban
            kanban_get_tasks,
            kanban_create_task,
            kanban_update_task,
            kanban_delete_task,
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
            // Mobile mirror relay
            relay_start,
            relay_stop,
            relay_status,
        ])
        .setup(|app| {
            // Request macOS notification permission up front so agent
            // alerts (finished / needs attention / error) can be delivered
            // natively without a first-use delay. No-op on other platforms.
            #[cfg(target_os = "macos")]
            {
                use tauri_plugin_notification::NotificationExt;
                if let Err(e) = app.notification().request_permission() {
                    log::warn!("failed to request notification permission: {e}");
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!("tauri.conf.json"))
        .expect("error while building tauri application");

    {
        let state = app.state::<state::AppState>();
        state.set_app_handle(app.handle().clone());
        state.wire_pty_events();

        // Mobile Mirror is an experimental plaintext LAN service and must
        // never silently reopen merely because it was enabled in an earlier
        // session. Settings activation remains explicit per process launch.
        // An operator may opt into boot auto-start for a trusted development
        // environment with ATHENA_RELAY_AUTOSTART=1; public builds leave this
        // unset, so the relay is fail-closed by default.
        let persisted_enabled = state
            .store
            .get::<String>(crate::commands::RELAY_ENABLED_KEY)
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        let explicit_autostart = relay_autostart_requested();
        let enabled = persisted_enabled && explicit_autostart;
        if enabled {
            let resource = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| std::path::PathBuf::new());
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
                .unwrap_or_default();
            let dist_dir = relay::resolve_dist_dir(&resource, &exe_dir);
            match relay_token() {
                Ok(token) => {
                    log::info!("[relay] relay.enabled=true — auto-starting on boot");
                    if let Err(e) = relay::start(app.handle().clone(), dist_dir, token) {
                        log::error!("[relay] auto-start failed: {e}");
                    }
                }
                Err(e) => log::error!("[relay] failed to load relay token: {e}"),
            }
        } else if persisted_enabled {
            log::info!("[relay] persisted enable found, but explicit ATHENA_RELAY_AUTOSTART=1 is required; skipping auto-start");
        } else {
            log::debug!("[relay] relay.enabled not set — skipping auto-start");
        }
    }

    app.run(|app_handle, event| {
        match event {
            tauri::RunEvent::Ready => {
                // The window-state plugin restores the saved geometry before
                // this point (in its `on_window_ready` hook). Guard against the
                // "lost window" failure: if the saved position was on a display
                // that is no longer connected (external monitor unplugged,
                // resolution changed), the plugin's one-corner intersection
                // test can still restore a mostly off-screen window. Re-center
                // it once the OS has applied the restored frame.
                if let Some(window) = app_handle.get_webview_window("main") {
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            WINDOW_OFFSCREEN_CHECK_DELAY_MS,
                        ))
                        .await;
                        ensure_window_on_screen(&window);
                    });
                }
            }
            tauri::RunEvent::ExitRequested { api: _, .. } => {
                log::info!("Exit requested -- initiating graceful shutdown");
                log::info!("[resume-debug] RunEvent::ExitRequested received; capturing resume hints before PTY shutdown");
                // Some Tauri/macOS paths deliver ExitRequested before Exit.
                // Capture while the PTYs are still alive; the Exit handler is
                // an idempotent fallback for platforms that skip this event.
                capture_resume_on_exit(app_handle);

                // Stop the mobile-mirror relay first so the port is
                // released before the runtime tears down. Idempotent.
                relay::stop();

                let state = app_handle.state::<state::AppState>();
                // Close native browser children before the main window/runtime
                // tears down. This keeps the model and Tauri registry aligned
                // even when the app exits without a browser hide command.
                shutdown_browser_children(&state);

                // Stop the dedicated MCP runtime and signal the TCP server
                // before attempting the synchronous cleanup lock. This keeps
                // shutdown reliable even if the async MCP mutex is contended.
                state
                    .mcp_runtime_stop
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                if let Ok(server) = state.mcp_server.try_lock() {
                    server.request_shutdown();
                }

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
                            log::info!("[resume-debug] ExitRequested PTY shutdown completed");
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
                // ExitRequested captures first when that event is delivered;
                // this remains an idempotent fallback for paths that deliver
                // only Exit. The PTYs are still alive when neither earlier
                // shutdown path has run.
                log::info!("[resume-debug] RunEvent::Exit received; invoking idempotent capture fallback");
                relay::stop();
                let state = app_handle.state::<state::AppState>();
                shutdown_browser_children(&state);
                state
                    .mcp_runtime_stop
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                if let Ok(server) = state.mcp_server.try_lock() {
                    server.request_shutdown();
                }
                if let Ok(mut server) = state.mcp_server.try_lock() {
                    server.shutdown();
                }
                capture_resume_on_exit(app_handle);
            }
            _ => {}
        }
    });
}

/// Minimum fraction of the window area that must land on a connected display
/// before the restored geometry is accepted. Windows below this are
/// re-centered. 25% keeps legitimate layouts — a window tucked partway off a
/// small laptop screen, or one straddling two monitors — while catching
/// windows stranded on a disconnected display.
const MIN_VISIBLE_WINDOW_FRACTION: f64 = 0.25;

/// How long after the event loop starts we wait before measuring the restored
/// window frame. The window-state plugin applies the geometry synchronously
/// before `RunEvent::Ready`, but macOS can defer the frame change until the
/// window server round-trips, so we let it settle first.
const WINDOW_OFFSCREEN_CHECK_DELAY_MS: u64 = 200;

/// Post-restore guard for the window-state plugin: the plugin only restores a
/// saved position if *one corner* of the window intersects a connected monitor
/// (see `MonitorExt::intersects` in tauri-plugin-window-state). After an
/// external display is unplugged, a window saved there can therefore restore
/// with just a sliver on-screen. If less than [`MIN_VISIBLE_WINDOW_FRACTION`]
/// of the window is visible across all monitors, re-center it on the primary
/// display. Best-effort and idempotent — never blocks startup or fails hard.
/// The area heuristic is deliberately simple: a window whose titlebar is
/// entirely off-screen but whose body is > 25% visible won't be rescued (rare,
/// and still grabbable via Mission Control), while the common unplugged-
/// display cases are caught.
fn ensure_window_on_screen(window: &tauri::WebviewWindow) {
    // A maximized or fullscreen window fills a monitor, so it can never be
    // stranded off-screen. Skip the geometry work entirely.
    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }

    let Ok(pos) = window.outer_position() else {
        log::warn!("[window] off-screen check: no outer position available");
        return;
    };
    let Ok(size) = window.outer_size() else {
        log::warn!("[window] off-screen check: no outer size available");
        return;
    };
    if size.width == 0 || size.height == 0 {
        return;
    }

    let win_l = pos.x as i64;
    let win_t = pos.y as i64;
    let win_r = win_l + size.width as i64;
    let win_b = win_t + size.height as i64;

    let monitors = match window.available_monitors() {
        Ok(monitors) => monitors
            .iter()
            .map(|m| {
                let m_pos = m.position();
                let m_size = m.size();
                (
                    m_pos.x as i64,
                    m_pos.y as i64,
                    m_pos.x as i64 + m_size.width as i64,
                    m_pos.y as i64 + m_size.height as i64,
                )
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            log::warn!("[window] off-screen check: failed to query monitors: {e}");
            return;
        }
    };

    let fraction = visible_fraction((win_l, win_t, win_r, win_b), &monitors);
    if fraction < MIN_VISIBLE_WINDOW_FRACTION {
        log::warn!(
            "[window] restored position is off-screen ({:.0}% visible) — re-centering",
            fraction * 100.0
        );
        if let Err(e) = window.center() {
            log::warn!("[window] failed to re-center window: {e}");
        }
    }
}

/// Fraction of a window frame (physical px, `(left, top, right, bottom)`) that
/// intersects the given monitors (each `(left, top, right, bottom)`). With
/// non-overlapping monitors this is the exact union fraction; overlapping
/// (e.g. mirrored) displays sum intersections, which only makes the rescue
/// heuristic less aggressive — acceptable for an off-screen guard.
fn visible_fraction(window: (i64, i64, i64, i64), monitors: &[(i64, i64, i64, i64)]) -> f64 {
    let (win_l, win_t, win_r, win_b) = window;
    let window_area = ((win_r - win_l).max(0) * (win_b - win_t).max(0)) as f64;
    if window_area == 0.0 {
        return 0.0;
    }
    let visible: i64 = monitors
        .iter()
        .map(|&(m_l, m_t, m_r, m_b)| {
            let w = (win_r.min(m_r) - win_l.max(m_l)).max(0);
            let h = (win_b.min(m_b) - win_t.max(m_t)).max(0);
            w * h
        })
        .sum();
    visible as f64 / window_area
}

static RESUME_CAPTURE_DONE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Keep macOS Cmd+Q responsive while still allowing fast agent exit handlers
/// to print and persist a resume id. The exit callback runs on Tauri's main
/// thread, so joining a worker beyond this budget makes the whole app appear
/// frozen with a spinning cursor.
const EXIT_RESUME_CAPTURE_BUDGET_MS: u64 = 800;

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
        log::debug!("[resume-debug] capture skipped: already completed in this process");
        return;
    }
    log::info!(
        "[resume-debug] capture worker starting with budget={}ms",
        EXIT_RESUME_CAPTURE_BUDGET_MS
    );
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
                let n = commands::capture_resume_ids_on_exit(&state, EXIT_RESUME_CAPTURE_BUDGET_MS)
                    .await;
                log::info!("[resume-debug] capture worker finished; persisted {n} resume id(s)");
            });
        });
    match worker {
        Ok(w) => {
            if let Err(e) = w.join() {
                log::error!("[resume-debug] capture worker panicked: {e:?}");
            }
        }
        Err(e) => log::error!("resume capture: failed to spawn worker thread: {e}"),
    }
}

#[cfg(test)]
mod window_geometry_tests {
    use super::visible_fraction;

    // A single built-in display: 0,0 → 1920×1080 (physical px).
    const PRIMARY: (i64, i64, i64, i64) = (0, 0, 1920, 1080);

    fn fraction(window: (i64, i64, i64, i64), monitors: &[(i64, i64, i64, i64)]) -> f64 {
        visible_fraction(window, monitors)
    }

    #[test]
    fn fully_inside_primary_is_fully_visible() {
        assert!((fraction((200, 100, 1600, 1000), &[PRIMARY]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sliver_after_monitor_unplugged_is_rescued() {
        // Saved on a second monitor to the right (x=1800, width 1400); that
        // display is gone, so only a 120px sliver overlaps the built-in screen.
        // This is the case the plugin's one-corner test accepts and we must
        // re-center (< 25% visible).
        let f = fraction((1800, 100, 3200, 1000), &[PRIMARY]);
        assert!(f < 0.25, "expected < 0.25, got {f}");
    }

    #[test]
    fn fully_offscreen_is_rescued() {
        let f = fraction((2560, 100, 3960, 1000), &[PRIMARY]);
        assert!(f.abs() < 1e-9);
        assert!(f < 0.25);
    }

    #[test]
    fn half_offscreen_is_kept() {
        // Deliberately half off the right edge (e.g. two windows side by side
        // on a small screen): 50% visible is above the threshold — no rescue.
        let f = fraction((960, 0, 2880, 1080), &[PRIMARY]);
        assert!((f - 0.5).abs() < 1e-9, "expected 0.5, got {f}");
        assert!(f >= 0.25);
    }

    #[test]
    fn titlebar_tucked_above_screen_is_kept() {
        // macOS commonly keeps a window whose titlebar is tucked just off the
        // top edge (-30px): 96% visible — no rescue.
        let f = fraction((100, -30, 1500, 870), &[PRIMARY]);
        assert!(f >= 0.25, "expected >= 0.25, got {f}");
    }

    #[test]
    fn window_on_connected_second_monitor_is_fully_visible() {
        let monitors = [PRIMARY, (1920, 0, 3840, 1080)];
        assert!((fraction((2000, 100, 3400, 1000), &monitors) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn window_straddling_two_connected_monitors_is_fully_visible() {
        let monitors = [PRIMARY, (1920, 0, 3840, 1080)];
        assert!((fraction((1800, 100, 3400, 1000), &monitors) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zero_area_window_is_zero() {
        assert!(fraction((100, 100, 100, 100), &[PRIMARY]).abs() < 1e-9);
    }
}
