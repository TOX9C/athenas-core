use crate::components::mobile_xterm::MobileXtermMount;
use crate::components::shared::icon::{
    IconAthena, IconChevronRight, IconClose, IconFiles, IconGrid, IconMenu, IconPlus, IconSeal,
    IconTerminal,
};
use crate::stores::workspace::{GridTemplate, PaneConfig, Space};
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
    web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .map(|hash| hash.contains("token=") && hash.trim_start_matches('#').len() > 6)
        .unwrap_or(false)
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

    use_effect(move || {
        if !token_present() {
            status.set("Pair this phone with an Athena desktop first".to_string());
            return;
        }
        spawn(async move {
            match tauri_bridge::store_get("workspaces").await {
                Ok(raw) => {
                    match serde_json::from_str::<crate::stores::workspace::WorkspaceState>(&raw) {
                        Ok(state) => {
                            let selected = state
                                .active_space_id
                                .clone()
                                .or_else(|| state.spaces.first().map(|space| space.id.clone()));
                            let pane = selected
                                .as_ref()
                                .and_then(|id| state.spaces.iter().find(|space| &space.id == id))
                                .and_then(|space| space.panes.first())
                                .map(|pane| pane.id.clone());
                            active_space.set(selected);
                            active_pane.set(pane);
                            spaces.set(state.spaces);
                            status.set("Connected to Athena".to_string());
                        }
                        Err(error) => status.set(format!("Workspace data is invalid: {error}")),
                    }
                }
                Err(error) => status.set(format!("Connection failed: {error:?}")),
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
        chat_input.set(String::new());
        chat_messages.write().push(MobileChatMessage {
            role: "You".to_string(),
            text: text.clone(),
        });
        busy.set(true);
        spawn(async move {
            match tauri_bridge::athena_chat(&text).await {
                Ok(reply) => chat_messages.write().push(MobileChatMessage {
                    role: "Athena".to_string(),
                    text: reply,
                }),
                Err(error) => chat_messages.write().push(MobileChatMessage {
                    role: "Error".to_string(),
                    text: format!("{error:?}"),
                }),
            }
            busy.set(false);
        });
    };

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
        let mut next = spaces.read().clone();
        next.push(space.clone());
        let state = crate::stores::workspace::WorkspaceState {
            spaces: next.clone(),
            active_space_id: Some(space.id.clone()),
        };
        status.set("Starting terminal…".to_string());
        show_new_space.set(false);
        new_space_name.set(String::new());
        new_space_dir.set(String::new());
        spawn(async move {
            let _ = tauri_bridge::workspace_add_trusted_root(&dir).await;
            let shell = tauri_bridge::pty_default_shell_cached().await;
            match tauri_bridge::pty_spawn(&pane_id, &dir, &shell, 100, 28, false, None).await {
                Ok(()) => {
                    active_pane.set(Some(pane_id));
                    active_screen.set(MobileScreen::Terminal);
                    active_space.set(Some(space.id.clone()));
                    spaces.set(state.spaces.clone());
                    let _ = tauri_bridge::store_set(
                        "workspaces",
                        &serde_json::to_string(&state).unwrap_or_default(),
                    )
                    .await;
                    status.set("Connected to Athena".to_string());
                }
                Err(error) => status.set(format!("Terminal start failed: {error:?}")),
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
                                    div { class: "mobile-terminal-frame",
                                        div { class: "mobile-terminal-frame-bar",
                                            span { class: "mobile-terminal-live-dot" }
                                            span { class: "mobile-terminal-session", "{pane_id}" }
                                            span { class: "mobile-terminal-live-label", "LIVE" }
                                        }
                                        MobileXtermMount { key: "mobile-xterm-{pane_id}", pane_id: pane_id, cwd: space.dir.clone() }
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
                                        let message_class = if message.role == "You" { "mobile-chat-message is-user" } else { "mobile-chat-message" };
                                        rsx! {
                                            div { key: "message-{index}", class: message_class,
                                                span { class: "mobile-message-role", "{message.role}" }
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
                                button { class: "mobile-primary-button mobile-ask-button", disabled: *busy.read(), onclick: move |_| send_chat(), if *busy.read() { "Thinking…" } else { "Ask" } }
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
                                div { class: "mobile-file-row", input { value: "{file_path}", placeholder: "/path/to/file", "aria-label": "File path", oninput: move |event| file_path.set(event.value()) } button { class: "mobile-ghost-button", onclick: move |_| { let path = file_path.read().clone(); let mut status_for_file = status; spawn(async move { match tauri_bridge::fs_read_file(&path).await { Ok(content) => file_content.set(content), Err(error) => status_for_file.set(format!("Read failed: {error:?}")) } }); }, "Read" } }
                                label { "File content" }
                                textarea { class: "mobile-file-editor", value: "{file_content}", placeholder: "File content", "aria-label": "File content", oninput: move |event| file_content.set(event.value()) }
                                button { class: "mobile-primary-button mobile-save-button", onclick: move |_| { let path = file_path.read().clone(); let content = file_content.read().clone(); let mut status_for_file = status; spawn(async move { match tauri_bridge::fs_write_file(&path, &content).await { Ok(_) => status_for_file.set("File saved".to_string()), Err(error) => status_for_file.set(format!("Save failed: {error:?}")) } }); }, "Save file" }
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
