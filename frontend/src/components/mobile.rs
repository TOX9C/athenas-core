use crate::components::mobile_xterm::MobileXtermMount;
use crate::components::shared::icon::{
    IconAthena, IconChevronRight, IconClose, IconFiles, IconGrid, IconMenu, IconPlus, IconSeal,
    IconTerminal,
};
use crate::stores::workspace::{GridTemplate, PaneConfig, Space, WorkspaceState};
use crate::tauri_bridge;
use dioxus::prelude::*;

fn mobile_entry_enabled() -> bool {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .map(|search| {
            search
                .split('&')
                .any(|part| part == "?mobile=1" || part == "mobile=1")
        })
        .unwrap_or(false)
}

fn token_present() -> bool {
    let window = web_sys::window();
    // The relay token rides in the query string (?token=...); accept the
    // legacy #token= fragment form too.
    let query_ok = window
        .as_ref()
        .and_then(|w| w.location().search().ok())
        .map(|search| search.contains("token=") && search.len() > 6)
        .unwrap_or(false);
    let hash_ok = window
        .and_then(|w| w.location().hash().ok())
        .map(|hash| hash.contains("token=") && hash.trim_start_matches('#').len() > 6)
        .unwrap_or(false);
    query_ok || hash_ok
}

pub fn should_render_mobile_app() -> bool {
    mobile_entry_enabled()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MobileScreen {
    Spaces,
    Terminal,
    Oracle,
    Files,
}

#[derive(Clone, Debug, PartialEq)]
struct MobilePane {
    id: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq)]
struct MobileChatMessage {
    role: String,
    text: String,
}

/// Parse a relay-forwarded event payload. On the desktop the Tauri bridge
/// hands listeners the payload string directly; over the relay the payload
/// arrives JSON-quoted one extra level (the backend emits String payloads,
/// and the relay forwards the quoted wire form verbatim). Parse twice when
/// the first pass yields a bare string.
fn parse_event_payload(payload: &str) -> Option<serde_json::Value> {
    match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(serde_json::Value::String(inner)) => serde_json::from_str(&inner).ok(),
        Ok(v) => Some(v),
        Err(_) => None,
    }
}

fn screen_title(screen: MobileScreen) -> &'static str {
    match screen {
        MobileScreen::Spaces => "Spaces",
        MobileScreen::Terminal => "Terminal",
        MobileScreen::Oracle => "Chat",
        MobileScreen::Files => "Files",
    }
}

#[component]
pub fn MobileApp() -> Element {
    let mut spaces = use_signal(Vec::<Space>::new);
    let mut active_space = use_signal(|| Option::<String>::None);
    let mut active_pane = use_signal(|| Option::<String>::None);
    let mut chat_input = use_signal(String::new);
    let mut chat_messages = use_signal(Vec::<MobileChatMessage>::new);
    let mut file_path = use_signal(String::new);
    let mut file_content = use_signal(String::new);
    let mut status = use_signal(|| "Connecting…".to_string());
    let mut busy = use_signal(|| false);
    let mut show_new_space = use_signal(|| false);
    let mut new_space_name = use_signal(String::new);
    let mut new_space_dir = use_signal(String::new);
    let mut active_screen = use_signal(|| MobileScreen::Spaces);
    let mut drawer_open = use_signal(|| false);
    // Streaming chat state: the in-flight relay request id and the id of the
    // chat session this phone is bound to (persisted in localStorage so a
    // page reload reattaches to the same conversation).
    let mut stream_request = use_signal(|| Option::<String>::None);
    let mut chat_session_id = use_signal(|| Option::<String>::None);

    use_effect(move || {
        if !token_present() {
            status.set("Pair this phone with an Athena desktop first".to_string());
            return;
        }
        // Load the chat session binding once (localStorage → existing session,
        // else a fresh "Athena Mobile" session row on the desktop backend).
        spawn(async move {
            let mut existing: Option<String> = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("athena.mobile.session_id").ok().flatten());
            if let Some(id) = &existing {
                // Verify the session still exists on the backend.
                let ok = tauri_bridge::session_get(id).await.is_ok();
                if !ok {
                    existing = None;
                }
            }
            let session_id: Option<String> = match existing {
                Some(id) => Some(id),
                None => match tauri_bridge::session_create(Some("Athena Mobile")).await {
                    Ok(raw) => {
                        let parsed = serde_json::from_str::<serde_json::Value>(&raw)
                            .ok()
                            .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(str::to_string));
                        parsed.inspect(|id| {
                            if let Some(storage) =
                                web_sys::window().and_then(|w| w.local_storage().ok().flatten())
                            {
                                let _ = storage.set_item("athena.mobile.session_id", id);
                            }
                        })
                    }
                    Err(error) => {
                        web_sys::console::warn_1(
                            &format!("[mobile] session create failed: {error:?}").into(),
                        );
                        None
                    }
                },
            };
            chat_session_id.set(session_id);
        });

        // Live stream of Athena chat events; routed by request id so stale
        // turns never mutate the visible conversation.
        if let Ok(_unlisten) = tauri_bridge::listen("athena:stream", move |payload: String| {
            let Some(event) = parse_event_payload(&payload) else {
                return;
            };
            let request_id = event
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let active_request = stream_request.read().clone();
            if request_id.is_empty() || active_request.as_deref() != Some(request_id) {
                return;
            }
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match event_type {
                "delta" => {
                    if let Some(text) = event.get("text").and_then(|v| v.as_str()) {
                        if let Some(last) = chat_messages.write().last_mut() {
                            if last.role == "Athena" {
                                last.text.push_str(text);
                            }
                        }
                    }
                }
                "completed" | "error" => {
                    if event_type == "error" {
                        let cancelled = event
                            .get("cancelled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if !cancelled {
                            let message = event
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Request failed");
                            if let Some(last) = chat_messages.write().last_mut() {
                                if last.role == "Athena" && last.text.is_empty() {
                                    last.role = "Error".to_string();
                                    last.text = message.to_string();
                                }
                            }
                        }
                    }
                    stream_request.set(None);
                    busy.set(false);
                }
                _ => {}
            }
        }) {
            // Listeners live for the app lifetime (same as the desktop panel).
        }

        // Workspace sync: any writer (desktop or another phone) that passes
        // through `store_set("workspaces")` triggers a reload here.
        if let Ok(_unlisten) = tauri_bridge::listen("workspace:changed", move |payload: String| {
            let state = parse_event_payload(&payload)
                .and_then(|v| serde_json::from_value::<WorkspaceState>(v).ok());
            if let Some(state) = state {
                let active = state
                    .active_space_id
                    .clone()
                    .filter(|id| state.spaces.iter().any(|s| &s.id == id));
                spaces.set(state.spaces);
                if let Some(id) = active {
                    let still_selected = active_space.read().is_some();
                    if !still_selected {
                        active_space.set(Some(id));
                    }
                } else {
                    active_space.set(None);
                    active_pane.set(None);
                }
            }
        }) {}

        spawn(async move {
            // Pairing approval is human-latency (the desktop operator must tap
            // Allow) — retry indefinitely with a live status rather than
            // leaving the companion dead until a manual reload.
            loop {
                match tauri_bridge::store_get("workspaces").await {
                    Ok(raw) => {
                        match serde_json::from_str::<crate::stores::workspace::WorkspaceState>(&raw)
                        {
                            Ok(state) => {
                                let selected = state
                                    .active_space_id
                                    .clone()
                                    .or_else(|| state.spaces.first().map(|space| space.id.clone()));
                                let pane = selected
                                    .as_ref()
                                    .and_then(|id| {
                                        state.spaces.iter().find(|space| &space.id == id)
                                    })
                                    .and_then(|space| space.panes.first())
                                    .map(|pane| pane.id.clone());
                                active_space.set(selected);
                                active_pane.set(pane);
                                spaces.set(state.spaces);
                                status.set("Connected to Athena".to_string());
                            }
                            Err(error) => {
                                web_sys::console::warn_1(
                                    &format!("[mobile] workspace parse failed: {error}").into(),
                                );
                                status.set(
                                    "Workspace data is invalid. Restart the app to resync."
                                        .to_string(),
                                );
                            }
                        }
                        break;
                    }
                    Err(error) => {
                        web_sys::console::warn_1(
                            &format!("[mobile] connect failed: {error:?}").into(),
                        );
                        status.set("Tap Allow on your Mac to finish pairing…".to_string());
                        gloo::timers::future::TimeoutFuture::new(1_500).await;
                    }
                }
            }
        });
    });

    let selected_space = spaces
        .read()
        .iter()
        .find(|space| Some(&space.id) == active_space.read().as_ref())
        .cloned();
    let panes: Vec<MobilePane> = selected_space
        .as_ref()
        .map(|space| {
            space
                .panes
                .iter()
                .map(|pane| MobilePane {
                    id: pane.id.clone(),
                    label: pane
                        .label
                        .clone()
                        .unwrap_or_else(|| pane.agent_type.to_string()),
                })
                .collect()
        })
        .unwrap_or_default();

    let mut select_space = move |space_id: String| {
        let pane = spaces
            .read()
            .iter()
            .find(|space| space.id == space_id)
            .and_then(|space| space.panes.first())
            .map(|pane| pane.id.clone());
        active_space.set(Some(space_id));
        active_pane.set(pane);
        active_screen.set(MobileScreen::Terminal);
        drawer_open.set(false);
    };

    let mut select_pane = move |pane_id: String| {
        active_pane.set(Some(pane_id));
    };

    let mut send_chat = move || {
        let text = chat_input.read().clone();
        if text.trim().is_empty() || *busy.read() {
            return;
        }
        let Some(session_id) = chat_session_id.read().clone() else {
            chat_messages.write().push(MobileChatMessage {
                role: "Error".to_string(),
                text: "Chat session not ready yet — retry in a moment.".to_string(),
            });
            return;
        };
        chat_input.set(String::new());
        chat_messages.write().push(MobileChatMessage {
            role: "You".to_string(),
            text: text.clone(),
        });
        // Placeholder the athena:stream deltas append into.
        chat_messages.write().push(MobileChatMessage {
            role: "Athena".to_string(),
            text: String::new(),
        });
        let request_id = format!("mobile-{}", uuid::Uuid::new_v4());
        stream_request.set(Some(request_id.clone()));
        busy.set(true);
        spawn(async move {
            if let Err(error) =
                tauri_bridge::athena_chat_stream(&text, &session_id, &request_id).await
            {
                chat_messages.write().push(MobileChatMessage {
                    role: "Error".to_string(),
                    text: format!("{error:?}"),
                });
                stream_request.set(None);
                busy.set(false);
            }
        });
    };

    let cancel_chat = move || {
        let Some(request_id) = stream_request.read().clone() else {
            return;
        };
        spawn(async move {
            let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
        });
    };

    // Read–mutate–save through the shared WorkspaceState type so phone-made
    // changes take the exact same persistence path as the desktop (coalesced
    // save queue → store_set → `workspace:changed` broadcast) instead of
    // writing the store by hand and racing the desktop's serializer.
    let mut save_new_space = move || {
        let name = new_space_name.read().trim().to_string();
        let dir = new_space_dir.read().trim().to_string();
        if name.is_empty() || dir.is_empty() {
            return;
        }
        let pane_id = format!("mobile-{}", uuid::Uuid::new_v4());
        let space = Space {
            id: format!("space-{}", uuid::Uuid::new_v4()),
            name,
            dir: dir.clone(),
            grid: GridTemplate::X1x1,
            panes: vec![PaneConfig {
                id: pane_id.clone(),
                agent_type: crate::types::workspace::AgentType::Shell,
                ..PaneConfig::default()
            }],
            color: "#c9a24b".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            last_opened_at: chrono::Utc::now().timestamp_millis(),
        };
        status.set("Starting terminal…".to_string());
        show_new_space.set(false);
        new_space_name.set(String::new());
        new_space_dir.set(String::new());
        spawn(async move {
            let _ = tauri_bridge::workspace_add_trusted_root(&dir).await;
            let mut state = WorkspaceState::load().await;
            state.add_space(space.clone());
            // Do NOT spawn the PTY here: `MobileXtermMount` is the single
            // spawn authority. Eager spawns emit the shell's first screen
            // before the mount's `pty:raw:` subscription registers
            // (`relay_raw_subscribers` is still 0), burning the prompt. The
            // mount spawns paused, subscribes, then resumes.
            active_pane.set(Some(pane_id));
            active_screen.set(MobileScreen::Terminal);
            active_space.set(Some(space.id.clone()));
            spaces.set(state.spaces.clone());
            status.set("Connected to Athena".to_string());
        });
    };

    let mut add_pane_to_active_space = move || {
        // Look up the space at tap time — capturing the derived
        // `selected_space` value would freeze the pane/dir at mount time.
        let Some(space_id) = active_space.read().clone() else {
            return;
        };
        if !spaces.read().iter().any(|s| s.id == space_id) {
            return;
        }
        let pane_id = format!("mobile-{}", uuid::Uuid::new_v4());
        status.set("Starting terminal…".to_string());
        spawn(async move {
            let mut state = WorkspaceState::load().await;
            state.add_pane_to_space(
                &space_id,
                PaneConfig {
                    id: pane_id.clone(),
                    agent_type: crate::types::workspace::AgentType::Shell,
                    ..PaneConfig::default()
                },
            );
            spaces.set(state.spaces.clone());
            // See save_new_space: no eager pty_spawn; the mount that observes
            // the new active_pane owns the paused-spawn/attach lifecycle.
            active_pane.set(Some(pane_id));
            status.set("Connected to Athena".to_string());
        });
    };

    let close_pane = move |space_id: String, pane_id: String| {
        spawn(async move {
            let _ = tauri_bridge::pty_kill(&pane_id).await;
            let mut state = WorkspaceState::load().await;
            state.remove_pane_from_space(&space_id, &pane_id);
            spaces.set(state.spaces.clone());
            if active_pane.read().as_deref() == Some(pane_id.as_str()) {
                active_pane.set(None);
            }
        });
    };

    let current_screen = active_screen();
    let title = screen_title(current_screen);

    rsx! {
        div { class: "mobile-app-shell",
            header { class: "mobile-topbar",
                button {
                    class: "mobile-icon-button mobile-menu-button",
                    "aria-label": "Open navigation",
                    title: "Open navigation",
                    onclick: move |_| drawer_open.set(true),
                    IconMenu { size: Some(22), color: Some("currentColor".to_string()) }
                }
                div { class: "mobile-brand",
                    span { class: "mobile-brand-seal",
                        IconSeal { size: Some(18), color: Some("var(--accent)".to_string()) }
                    }
                    span { "Athena" }
                }
                div { class: "mobile-topbar-right",
                    span { class: "mobile-status-dot", title: "{status}" }
                    span { class: "mobile-status-text", "{status}" }
                    span { class: "mobile-screen-title", "{title}" }
                }
            }

            if !token_present() {
                section { class: "mobile-pair-card",
                    div { class: "mobile-eyebrow", "COMPANION APP" }
                    h1 { "Connect to your Athena desktop." }
                    p { "Open Athena on your Mac, enable Mobile Mirror, and use its QR code or connection link on the same Wi-Fi." }
                    a { class: "mobile-primary-button", href: "/mobile.html", "Open pairing" }
                    div { class: "mobile-pair-note", "No desktop connection token found in this tab." }
                }
            } else {
                main { class: "mobile-main",
                    if current_screen == MobileScreen::Spaces {
                        section { class: "mobile-screen mobile-scroll-screen mobile-spaces-screen",
                            div { class: "mobile-screen-heading",
                                div { class: "mobile-eyebrow", "WORKSPACE" }
                                div { class: "mobile-heading-row",
                                    h1 { "Your spaces" }
                                    button {
                                        class: "mobile-icon-button mobile-accent-icon-button",
                                        "aria-label": "Create workspace",
                                        title: "Create workspace",
                                        onclick: move |_| show_new_space.set(true),
                                        IconPlus { size: Some(18), color: Some("currentColor".to_string()) }
                                    }
                                }
                                p { "Choose a workspace to continue where you left off." }
                            }
                            div { class: "mobile-space-list",
                                for space in spaces.read().iter() {
                                    {
                                        let id = space.id.clone();
                                        let is_active = active_space.read().as_deref() == Some(space.id.as_str());
                                        rsx! {
                                            button {
                                                key: "space-{id}",
                                                class: if is_active { "mobile-space-card is-active" } else { "mobile-space-card" },
                                                onclick: move |_| select_space(id.clone()),
                                                span { class: "mobile-space-card-icon", IconGrid { size: Some(19), color: Some("currentColor".to_string()) } }
                                                span { class: "mobile-space-card-copy",
                                                    strong { "{space.name}" }
                                                    small { "{space.panes.len()} panes" }
                                                    small { class: "mobile-space-path", "{space.dir}" }
                                                }
                                                span { class: "mobile-space-card-arrow",
                                                    IconChevronRight { size: Some(15), color: Some("var(--textDim)".to_string()) }
                                                }
                                            }
                                        }
                                    }
                                }
                                if spaces.read().is_empty() {
                                    div { class: "mobile-empty-card",
                                        IconGrid { size: Some(28), color: Some("var(--accent)".to_string()) }
                                        strong { "No workspaces yet" }
                                        span { "Create one to start working from your phone." }
                                        button { class: "mobile-primary-button", onclick: move |_| show_new_space.set(true), "Create workspace" }
                                    }
                                }
                            }
                        }
                    } else if current_screen == MobileScreen::Terminal {
                        section { class: "mobile-screen mobile-terminal-screen",
                            div { class: "mobile-screen-heading mobile-terminal-heading",
                                div { class: "mobile-terminal-meta",
                                    div { class: "mobile-eyebrow", "TMUX / {panes.len()} PANES" }
                                    div { class: "mobile-heading-row",
                                        div { class: "mobile-heading-with-icon", IconTerminal { size: Some(20), color: Some("var(--accent)".to_string()) } h1 { "Terminal" } }
                                        if let Some(space) = selected_space.as_ref() { span { class: "mobile-terminal-cwd", "{space.dir}" } }
                                    }
                                }
                                if let Some(space) = selected_space.as_ref() { p { "{space.name} · live PTY" } } else { p { "Select a workspace from Spaces." } }
                            }
                            if !panes.is_empty() {
                                div { class: "mobile-pane-tabs",
                                    button {
                                        class: "mobile-pane-tab mobile-pane-add",
                                        "aria-label": "New terminal",
                                        title: "New terminal",
                                        onclick: move |_| add_pane_to_active_space(),
                                        IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                                    }
                                    for (index, pane) in panes.iter().enumerate() {
                                        {
                                            let id = pane.id.clone();
                                            let pane_class = if active_pane.read().as_deref() == Some(id.as_str()) { "mobile-pane-tab is-active" } else { "mobile-pane-tab" };
                                            rsx! {
                                                button { key: "pane-{id}", class: pane_class, onclick: move |_| select_pane(id.clone()),
                                                    span { class: "mobile-pane-index", "{index + 1}" }
                                                    span { "{pane.label}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if let (Some(pane_id), Some(space)) = (active_pane.read().clone(), selected_space.as_ref()) {
                                    {
                                        let request_pane = pane_id.clone();
                                        rsx! {
                                            div { key: "mobile-terminal-frame-{pane_id}", class: "mobile-terminal-frame",
                                                {
                                                    let pane_id_full = pane_id.clone();
                                                    let pane_id_short: String =
                                                        pane_id.chars().take(10).collect();
                                                    let space_for_close = space.id.clone();
                                                    let pane_for_close = pane_id.clone();
                                                    rsx! {
                                                        div { class: "mobile-terminal-frame-bar",
                                                            span { class: "mobile-terminal-live-dot" }
                                                            span { class: "mobile-terminal-session", title: "{pane_id_full}", "{pane_id_short}…" }
                                                            button {
                                                                class: "mobile-frame-close-button",
                                                                "aria-label": "Close terminal",
                                                                title: "Close terminal",
                                                                onclick: move |_| close_pane(space_for_close.clone(), pane_for_close.clone()),
                                                                IconClose { size: Some(13), color: Some("currentColor".to_string()) }
                                                            }
                                                            span { class: "mobile-terminal-live-label", "LIVE" }
                                                        }
                                                        button {
                                                            class: "mobile-frame-request-button",
                                                            onclick: move |_| {
                                                                let pane_id = request_pane.clone();
                                                                spawn(async move {
                                                                    let _ = tauri_bridge::relay_request_pane_share(&pane_id).await;
                                                                });
                                                            },
                                                            "Request access"
                                                        }
                                                        MobileXtermMount { pane_id: pane_id, cwd: space.dir.clone() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "mobile-terminal-empty", IconTerminal { size: Some(26), color: Some("var(--accent)".to_string()) } "Choose a pane to attach." }
                                }
                            } else {
                                div { class: "mobile-terminal-empty", IconTerminal { size: Some(26), color: Some("var(--accent)".to_string()) } "This workspace has no panes." }
                            }
                        }
                    } else if current_screen == MobileScreen::Oracle {
                        section { class: "mobile-screen mobile-chat-screen",
                            div { class: "mobile-screen-heading",
                                div { class: "mobile-heading-with-icon", IconAthena { size: Some(21), color: Some("var(--accent)".to_string()) } h1 { "Ask Athena" } }
                                p { "Chat with the same Athena session running on your Mac." }
                            }
                            div { class: "mobile-chat-log mobile-scroll-screen",
                                for (index, message) in chat_messages.read().iter().enumerate() {
                                    {
                                        let message_class = if message.role == "You" {
                                            "mobile-chat-message is-user"
                                        } else if message.role == "Error" {
                                            "mobile-chat-message is-error"
                                        } else {
                                            "mobile-chat-message"
                                        };
                                        rsx! {
                                            // No sender label — position alone
                                            // distinguishes You vs Athena.
                                            div { key: "message-{index}", class: message_class,
                                                p { "{message.text}" }
                                            }
                                        }
                                    }
                                }
                                if chat_messages.read().is_empty() {
                                    div { class: "mobile-chat-empty",
                                        IconAthena { size: Some(26), color: Some("var(--accent)".to_string()) }
                                        span { "Ask for a code change, inspect an agent, or plan your next move." }
                                    }
                                }
                            }
                            div { class: "mobile-chat-compose",
                                textarea { value: "{chat_input}", placeholder: "What should Athena do?", "aria-label": "Message Athena", oninput: move |event| chat_input.set(event.value()), onkeydown: move |event| if event.key() == Key::Enter && !event.modifiers().shift() { send_chat(); } }
                                if *busy.read() {
                                    button { class: "mobile-ghost-button mobile-ask-button", onclick: move |_| cancel_chat(), "Stop" }
                                } else {
                                    button { class: "mobile-primary-button mobile-ask-button", onclick: move |_| send_chat(), "Ask" }
                                }
                            }
                        }
                    } else {
                        section { class: "mobile-screen mobile-scroll-screen mobile-files-screen",
                            div { class: "mobile-screen-heading",
                                div { class: "mobile-heading-with-icon", IconFiles { size: Some(21), color: Some("var(--accent)".to_string()) } h1 { "Files" } }
                                p { "Read and save files inside the selected workspace." }
                            }
                            div { class: "mobile-file-form",
                                label { "File path" }
                                div { class: "mobile-file-row", input { value: "{file_path}", placeholder: "/path/to/file", "aria-label": "File path", oninput: move |event| file_path.set(event.value()) } button { class: "mobile-ghost-button", onclick: move |_| { let path = file_path.read().clone(); let mut status_for_file = status; spawn(async move { match tauri_bridge::fs_read_file(&path).await { Ok(content) => file_content.set(content),                Err(error) => {
                    web_sys::console::warn_1(&format!("[mobile] read failed: {error:?}").into());
                    status_for_file.set("Couldn't read the file.".to_string());
                } } }); }, "Read" } }
                                label { "File content" }
                                textarea { class: "mobile-file-editor", value: "{file_content}", placeholder: "File content", "aria-label": "File content", oninput: move |event| file_content.set(event.value()) }
                                button { class: "mobile-primary-button mobile-save-button", onclick: move |_| { let path = file_path.read().clone(); let content = file_content.read().clone(); let mut status_for_file = status; spawn(async move { match tauri_bridge::fs_write_file(&path, &content).await { Ok(_) => status_for_file.set("File saved".to_string()),                Err(error) => {
                    web_sys::console::warn_1(&format!("[mobile] save failed: {error:?}").into());
                    status_for_file.set("Couldn't save the file.".to_string());
                } } }); }, "Save file" }
                            }
                        }
                    }
                }
            }

            if drawer_open() {
                div { class: "mobile-drawer-layer",
                    div { class: "mobile-drawer-backdrop", onclick: move |_| drawer_open.set(false) }
                    nav { class: "mobile-drawer", role: "navigation", "aria-label": "Mobile navigation",
                        div { class: "mobile-drawer-header",
                            div { class: "mobile-brand",
                                span { class: "mobile-brand-seal",
                                    IconSeal { size: Some(18), color: Some("var(--accent)".to_string()) }
                                }
                                span { "Athena" }
                            }
                            button { class: "mobile-icon-button", "aria-label": "Close navigation", onclick: move |_| drawer_open.set(false), IconClose { size: Some(20), color: Some("currentColor".to_string()) } }
                        }
                        div { class: "mobile-drawer-status", span { class: "mobile-live-dot" } "{status}" }
                        div { class: "mobile-nav-list",
                            button { class: if current_screen == MobileScreen::Spaces { "mobile-nav-item is-active" } else { "mobile-nav-item" }, onclick: move |_| { active_screen.set(MobileScreen::Spaces); drawer_open.set(false); }, IconGrid { size: Some(19), color: Some("currentColor".to_string()) } span { "Spaces" } }
                            button { class: if current_screen == MobileScreen::Terminal { "mobile-nav-item is-active" } else { "mobile-nav-item" }, onclick: move |_| { active_screen.set(MobileScreen::Terminal); drawer_open.set(false); }, IconTerminal { size: Some(19), color: Some("currentColor".to_string()) } span { "Terminal" } }
                            button { class: if current_screen == MobileScreen::Oracle { "mobile-nav-item is-active" } else { "mobile-nav-item" }, onclick: move |_| { active_screen.set(MobileScreen::Oracle); drawer_open.set(false); }, IconAthena { size: Some(19), color: Some("currentColor".to_string()) } span { "Chat" } }
                            button { class: if current_screen == MobileScreen::Files { "mobile-nav-item is-active" } else { "mobile-nav-item" }, onclick: move |_| { active_screen.set(MobileScreen::Files); drawer_open.set(false); }, IconFiles { size: Some(19), color: Some("currentColor".to_string()) } span { "Files" } }
                        }
                        div { class: "mobile-drawer-footer",
                            span { class: "mobile-drawer-footer-label", "DESKTOP LINK" }
                            span { class: "mobile-drawer-footer-copy", "Connected over your private LAN" }
                        }
                    }
                }
            }

            if *show_new_space.read() {
                div { class: "mobile-modal-backdrop", onclick: move |_| show_new_space.set(false),
                    div { class: "mobile-modal-card", onclick: move |event| event.stop_propagation(),
                        div { class: "mobile-heading-row", h2 { "New workspace" } button { class: "mobile-icon-button", "aria-label": "Close dialog", onclick: move |_| show_new_space.set(false), IconClose { size: Some(18), color: Some("currentColor".to_string()) } } }
                        label { "Workspace name" }
                        input { placeholder: "Workspace name", value: "{new_space_name}", "aria-label": "Workspace name", oninput: move |event| new_space_name.set(event.value()) }
                        label { "Working directory" }
                        input { placeholder: "/working/directory", value: "{new_space_dir}", "aria-label": "Working directory", oninput: move |event| new_space_dir.set(event.value()) }
                        button { class: "mobile-primary-button", onclick: move |_| save_new_space(), "Create workspace" }
                    }
                }
            }
        }
    }
}
