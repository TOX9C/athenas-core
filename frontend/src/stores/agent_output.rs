use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single line of agent output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: usize,
    pub timestamp: i64,
    pub text: String,
}

/// Metadata about an agent's output stream.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentOutputInfo {
    pub pane_id: String,
    pub agent_type: String,
    pub line_count: usize,
    pub created_at: i64,
    pub last_activity_at: i64,
}

/// State of a subscription to a pane's output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubscriptionState {
    pub subscription_id: Option<String>,
    pub pane_id: Option<String>,
    pub active: bool,
}

/// Maximum lines retained per output buffer.
const MAX_LINES_PER_BUFFER: usize = 5000;
/// Maximum characters retained per output line (prevents DoS from huge single lines).
const MAX_TEXT_LENGTH: usize = 10000;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global agent output tracking state.
#[derive(Clone, PartialEq, Default)]
pub struct AgentOutputState {
    pub buffers: Vec<(String, Vec<OutputLine>)>,
    pub agents: Vec<AgentOutputInfo>,
    pub selected_pane_id: Option<String>,
    pub subscription: SubscriptionState,
    pub inspector_open: bool,
    pub auto_scroll: bool,
}

impl AgentOutputState {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            agents: Vec::new(),
            selected_pane_id: None,
            subscription: SubscriptionState {
                subscription_id: None,
                pane_id: None,
                active: false,
            },
            inspector_open: false,
            auto_scroll: true,
        }
    }

    // -- Helpers -----------------------------------------------------------

    fn find_buffer_mut(&mut self, pane_id: &str) -> Option<&mut Vec<OutputLine>> {
        self.buffers
            .iter_mut()
            .find(|(id, _)| id == pane_id)
            .map(|(_, lines)| lines)
    }

    fn trim_lines(lines: &mut Vec<OutputLine>) {
        if lines.len() > MAX_LINES_PER_BUFFER {
            let excess = lines.len() - MAX_LINES_PER_BUFFER;
            lines.drain(0..excess);
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn set_lines(&mut self, pane_id: impl Into<String>, lines: Vec<OutputLine>) {
        let key = pane_id.into();
        let mut trimmed = lines;
        Self::trim_lines(&mut trimmed);
        if let Some(buf) = self.find_buffer_mut(&key) {
            *buf = trimmed;
        } else {
            self.buffers.push((key, trimmed));
        }
    }

    pub fn append_line(&mut self, mut line: OutputLine) {
        let pane_id = line.pane_id.clone();
        // Truncate overly long lines to prevent memory spikes
        if line.text.len() > MAX_TEXT_LENGTH {
            line.text.truncate(MAX_TEXT_LENGTH);
        }
        if let Some(buf) = self.find_buffer_mut(&pane_id) {
            buf.push(line);
            Self::trim_lines(buf);
        } else {
            self.buffers.push((pane_id, vec![line]));
        }
    }

    pub fn clear_buffer(&mut self, pane_id: &str) {
        self.buffers.retain(|(id, _)| id != pane_id);
    }

    pub fn set_agents(&mut self, agents: Vec<AgentOutputInfo>) {
        self.agents = agents;
    }

    pub fn select_agent(&mut self, pane_id: Option<String>) {
        self.selected_pane_id = pane_id;
    }

    pub fn set_subscription(&mut self, sub: SubscriptionState) {
        self.subscription = sub;
    }

    pub fn clear_subscription(&mut self) {
        self.subscription = SubscriptionState {
            subscription_id: None,
            pane_id: None,
            active: false,
        };
    }

    pub fn set_inspector_open(&mut self, open: bool) {
        self.inspector_open = open;
    }

    pub fn set_auto_scroll(&mut self, auto: bool) {
        self.auto_scroll = auto;
    }

    // -- Event handlers for Tauri push events -------------------------------

    /// Register a new output pane.
    pub fn register_pane(&mut self, pane_id: String, agent_type: String, now: i64) {
        if !self.agents.iter().any(|a| a.pane_id == pane_id) {
            self.agents.push(AgentOutputInfo {
                pane_id: pane_id.clone(),
                agent_type,
                line_count: 0,
                created_at: now,
                last_activity_at: now,
            });
            // Ensure buffer exists
            if !self.buffers.iter().any(|(id, _)| id == &pane_id) {
                self.buffers.push((pane_id, Vec::new()));
            }
        }
    }

    /// Unregister an output pane.
    pub fn unregister_pane(&mut self, pane_id: &str) {
        self.agents.retain(|a| a.pane_id != pane_id);
        self.buffers.retain(|(id, _)| id != pane_id);
        if self.selected_pane_id.as_deref() == Some(pane_id) {
            self.selected_pane_id = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the agent output signal from the Dioxus context.
pub fn use_agent_output_store() -> Signal<AgentOutputState> {
    use_context::<Signal<AgentOutputState>>()
}

/// Initialize the agent output store as a context provider.
pub fn provide_agent_output_store() {
    use_context_provider(|| Signal::new(AgentOutputState::new()));
}
