use dioxus::prelude::*;

use crate::tauri_bridge::store_get as kv_get;
use crate::tauri_bridge::store_set as kv_set;
use std::collections::HashMap;
use std::rc::Rc;

#[path = "command_filter.rs"]
mod command_filter;

pub use command_filter::{filter_commands, CommandGroup};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Command category for grouping in the palette.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CommandCategory {
    #[default]
    Workspace,
    Panel,
    Athena,
    Terminal,
    File,
    Settings,
    Navigation,
}

/// A registrable command in the command palette.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub category: CommandCategory,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    /// The handler is stored as an index into a callback table because
    /// closures are not `PartialEq`. In the Dioxus integration layer the
    /// actual handler invocation is done via a separate registry.
    pub handler_key: String,
    /// Optional visibility predicate key (same approach as handler).
    pub when_key: Option<String>,
}

/// Maximum number of recent command ids retained.
const MAX_RECENT: usize = 8;

/// Key used in KeyValueStore for recent command ids persistence.
const RECENT_KEY: &str = "command_recent";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global command palette state.
#[derive(Clone, PartialEq, Default)]
pub struct CommandState {
    pub is_open: bool,
    pub query: String,
    pub commands: Vec<Command>,
    pub recent_ids: Vec<String>,
}

impl CommandState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            commands: Vec::new(),
            recent_ids: Vec::new(),
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
    }

    pub fn toggle(&mut self) {
        let was_open = self.is_open;
        self.is_open = !was_open;
        if self.is_open {
            self.query.clear();
        }
    }

    pub fn set_query(&mut self, q: impl Into<String>) {
        self.query = q.into();
    }

    /// Register a single command. No-op if a command with the same id exists.
    pub fn register_command(&mut self, cmd: Command) {
        if !self.commands.iter().any(|c| c.id == cmd.id) {
            self.commands.push(cmd);
        }
    }

    /// Unregister a command by id.
    pub fn unregister_command(&mut self, id: &str) {
        self.commands.retain(|c| c.id != id);
        self.recent_ids.retain(|rid| rid != id);
    }

    /// Bulk-register commands, skipping duplicates.
    pub fn register_commands(&mut self, cmds: Vec<Command>) {
        let existing_ids: std::collections::HashSet<String> =
            self.commands.iter().map(|c| c.id.clone()).collect();
        for cmd in cmds {
            if !existing_ids.contains(&cmd.id) {
                self.commands.push(cmd);
            }
        }
    }

    /// Record a command execution in the recent list.
    /// Persists the updated list to the backend KeyValueStore so the
    /// recents survive an app restart.
    pub fn record_execution(&mut self, id: &str) {
        self.recent_ids.retain(|rid| rid != id);
        self.recent_ids.insert(0, id.to_string());
        self.recent_ids.truncate(MAX_RECENT);

        // Persist the updated list. Best-effort: a failure to save is
        // logged but does not disrupt the in-memory state.
        let json = match serde_json::to_string(&self.recent_ids) {
            Ok(j) => j,
            Err(e) => {
                web_sys::console::error_1(&format!("[CommandState] serialize error: {}", e).into());
                return;
            }
        };

        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = kv_set(RECENT_KEY, &json).await {
                web_sys::console::error_1(
                    &format!("[CommandState] store_set error: {:?}", e).into(),
                );
            }
        });
    }

    /// Load the persisted recent ids from the backend KeyValueStore.
    /// Returns an empty vec if nothing is saved or the payload is
    /// malformed; logs (but does not propagate) deserialization errors.
    pub async fn load_recent() -> Vec<String> {
        match kv_get(RECENT_KEY).await {
            Ok(json) => {
                if json.trim().is_empty() {
                    return Vec::new();
                }
                match serde_json::from_str::<Vec<String>>(&json) {
                    Ok(ids) => ids,
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("[CommandState] deserialize error: {}", e).into(),
                        );
                        Vec::new()
                    }
                }
            }
            Err(_) => {
                // Key absent on first run — not an error.
                Vec::new()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the command palette signal from the Dioxus context.
pub fn use_command_store() -> Signal<CommandState> {
    use_context::<Signal<CommandState>>()
}

/// Initialize the command store as a context provider.
pub fn provide_command_store() {
    use_context_provider(|| Signal::new(CommandState::new()));
}

/// Handler callback table keyed by [`Command::handler_key`]. The `Command`
/// struct stores a key instead of a closure (closures are not `PartialEq`);
/// the actual invocation happens through this registry. Provided once by the
/// root component, which owns the signals the handlers mutate.
#[derive(Clone, Default)]
pub struct CommandHandlers(pub Rc<HashMap<String, Callback<()>>>);

/// Obtain the command handler registry from the Dioxus context.
pub fn use_command_handlers() -> CommandHandlers {
    use_context::<CommandHandlers>()
}

/// Dispatch the handler registered for `handler_key`, if any.
pub fn dispatch_command(handlers: &CommandHandlers, handler_key: &str) {
    match handlers.0.get(handler_key) {
        Some(handler) => handler.call(()),
        None => {
            web_sys::console::warn_1(
                &format!("Command palette: no handler registered for '{handler_key}'").into(),
            );
        }
    }
}
