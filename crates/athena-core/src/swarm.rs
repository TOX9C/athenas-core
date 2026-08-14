//! Swarm coordinator: persisted multi-agent mission state and mailboxes.
//!
//! The swarm state file is the source of truth shared by the desktop UI and
//! agent processes. State writes are serialized per workspace and committed
//! atomically; mailbox writes use an OS advisory lock so separate processes
//! cannot lose messages.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::EventEmitter;

const STALLED_TIMEOUT_MS: u64 = 90_000;
const MAX_ID_BYTES: usize = 128;
const MAX_CONTENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum SwarmError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Lock poisoned")]
    LockPoisoned,
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid swarm identifier: {0}")]
    InvalidIdentifier(String),
    #[error("Content exceeds the {0} byte limit")]
    ContentTooLarge(usize),
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn generate_id(prefix: &str) -> String {
    format!("{}-{}", prefix, uuid::Uuid::new_v4())
}

fn validate_identifier(value: &str) -> Result<(), SwarmError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(|c| c.is_control())
    {
        return Err(SwarmError::InvalidIdentifier(value.to_string()));
    }
    Ok(())
}

fn validate_content(value: &str) -> Result<(), SwarmError> {
    if value.len() > MAX_CONTENT_BYTES {
        return Err(SwarmError::ContentTooLarge(MAX_CONTENT_BYTES));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwarmAgent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub agent_type: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default = "default_agent_status")]
    pub status: String,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub last_action: String,
    #[serde(default)]
    pub last_action_at: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_agent_status() -> String {
    "idle".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwarmTask {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub assigned_agent_id: String,
    #[serde(default)]
    pub owned_files: Vec<String>,
    #[serde(default = "default_task_status")]
    pub status: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub last_updated_at: i64,
}

fn default_task_status() -> String {
    "queued".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MailboxMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub read: bool,
}

/// Complete persisted mission state. `extra` preserves fields written by
/// older/third-party agents so upgrades do not destroy information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwarmState {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub workspace_dir: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub agents: Vec<SwarmAgent>,
    #[serde(default)]
    pub tasks: Vec<SwarmTask>,
    #[serde(default)]
    pub messages: Vec<MailboxMessage>,
    #[serde(default = "default_swarm_status")]
    pub status: String,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub revision: u64,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_swarm_status() -> String {
    "active".to_string()
}

impl Default for SwarmState {
    fn default() -> Self {
        Self {
            id: String::new(),
            workspace_dir: String::new(),
            goal: String::new(),
            agents: Vec::new(),
            tasks: Vec::new(),
            messages: Vec::new(),
            status: default_swarm_status(),
            started_at: 0,
            revision: 0,
            extra: HashMap::new(),
        }
    }
}

pub struct SwarmCoordinator {
    watch_tx: Option<watch::Sender<SwarmState>>,
    watch_rx: watch::Receiver<SwarmState>,
    cancel_tokens: Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    watch_generations: Arc<Mutex<HashMap<String, u64>>>,
    next_watch_generation: Arc<AtomicU64>,
    state_locks: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    event_emitter: EventEmitter,
}

impl std::fmt::Debug for SwarmCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwarmCoordinator").finish()
    }
}

impl Clone for SwarmCoordinator {
    fn clone(&self) -> Self {
        Self {
            watch_tx: self.watch_tx.clone(),
            watch_rx: self.watch_rx.clone(),
            cancel_tokens: self.cancel_tokens.clone(),
            watch_generations: self.watch_generations.clone(),
            next_watch_generation: self.next_watch_generation.clone(),
            state_locks: self.state_locks.clone(),
            event_emitter: self.event_emitter.clone(),
        }
    }
}

impl Default for SwarmCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SwarmCoordinator {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(SwarmState::default());
        Self {
            watch_tx: Some(tx),
            watch_rx: rx,
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            watch_generations: Arc::new(Mutex::new(HashMap::new())),
            next_watch_generation: Arc::new(AtomicU64::new(0)),
            state_locks: Arc::new(Mutex::new(HashMap::new())),
            event_emitter: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    fn state_lock(&self, dir: &str) -> Result<Arc<tokio::sync::Mutex<()>>, SwarmError> {
        let mut locks = self
            .state_locks
            .lock()
            .map_err(|_| SwarmError::LockPoisoned)?;
        Ok(locks
            .entry(dir.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }

    fn emit_state(&self, state: &SwarmState) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                if let Ok(value) = serde_json::to_value(state) {
                    emitter("swarm:stateChange", &value);
                }
            }
        }
    }

    async fn read_state_unlocked(&self, dir: &str) -> Result<Option<SwarmState>, SwarmError> {
        let path = PathBuf::from(dir).join(".ade").join("swarm-state.json");
        match fs::read_to_string(path).await {
            Ok(content) => Ok(Some(serde_json::from_str(&content)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SwarmError::Io(error)),
        }
    }

    async fn write_state_unlocked(&self, dir: &str, state: &SwarmState) -> Result<(), SwarmError> {
        let ade_dir = PathBuf::from(dir).join(".ade");
        fs::create_dir_all(&ade_dir).await?;
        let state_path = ade_dir.join("swarm-state.json");
        // UUIDs avoid collisions when multiple writes occur within one ms.
        let tmp_path = ade_dir.join(format!("swarm-state.json.tmp.{}", uuid::Uuid::new_v4()));
        let content = serde_json::to_string_pretty(state)?;
        let mut file = fs::File::create(&tmp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&tmp_path, &state_path).await?;
        Ok(())
    }

    pub async fn read_state(&self, dir: &str) -> Result<Option<SwarmState>, SwarmError> {
        self.read_state_unlocked(dir).await
    }

    pub async fn write_state(&self, dir: &str, state: &SwarmState) -> Result<(), SwarmError> {
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        self.write_state_unlocked(dir, state).await
    }

    /// Create or replace a mission, persist it, emit its complete state, and
    /// begin stalled-agent monitoring for the workspace.
    pub async fn create_swarm(
        &self,
        dir: &str,
        mut state: SwarmState,
    ) -> Result<SwarmState, SwarmError> {
        validate_identifier(&state.id)?;
        validate_content(dir)?;
        state.workspace_dir = dir.to_string();
        validate_content(&state.goal)?;
        for agent in &state.agents {
            validate_identifier(&agent.id)?;
            validate_identifier(&agent.pane_id)?;
        }
        state.revision = state.revision.saturating_add(1).max(1);
        if state.started_at == 0 {
            state.started_at = now_ms();
        }
        if state.status.is_empty() {
            state.status = "active".to_string();
        }
        self.write_state(dir, &state).await?;
        self.emit_state(&state);
        self.watch_state(dir).await?;
        Ok(state)
    }

    pub async fn update_agent(
        &self,
        dir: &str,
        agent_id: &str,
        status: Option<String>,
        last_action: Option<String>,
        current_task: Option<Option<String>>,
    ) -> Result<SwarmState, SwarmError> {
        validate_identifier(agent_id)?;
        if let Some(ref action) = last_action {
            validate_content(action)?;
        }
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        let mut state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::AgentNotFound(agent_id.to_string()))?;
        let agent = state
            .agents
            .iter_mut()
            .find(|agent| agent.id == agent_id)
            .ok_or_else(|| SwarmError::AgentNotFound(agent_id.to_string()))?;
        if let Some(value) = status {
            if !matches!(
                value.as_str(),
                "idle" | "thinking" | "writing" | "waiting" | "done" | "blocked" | "stalled"
            ) {
                return Err(SwarmError::InvalidIdentifier(value));
            }
            agent.status = value;
        }
        if let Some(value) = last_action {
            agent.last_action = value;
        }
        if let Some(value) = current_task {
            agent.current_task = value;
        }
        agent.last_action_at = now_ms();
        state.revision = state.revision.saturating_add(1);
        self.write_state_unlocked(dir, &state).await?;
        drop(_guard);
        self.emit_state(&state);
        Ok(state)
    }

    pub async fn set_status(&self, dir: &str, status: &str) -> Result<SwarmState, SwarmError> {
        if !matches!(status, "active" | "paused" | "completed" | "cancelled") {
            return Err(SwarmError::InvalidIdentifier(status.to_string()));
        }
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        let mut state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::InvalidIdentifier("swarm state does not exist".into()))?;
        state.status = status.to_string();
        state.revision = state.revision.saturating_add(1);
        self.write_state_unlocked(dir, &state).await?;
        drop(_guard);
        self.emit_state(&state);
        Ok(state)
    }

    pub async fn create_task(
        &self,
        dir: &str,
        title: String,
        description: String,
        assigned_agent_id: String,
    ) -> Result<SwarmState, SwarmError> {
        validate_content(&title)?;
        validate_content(&description)?;
        validate_identifier(&assigned_agent_id)?;
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        let mut state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::InvalidIdentifier("swarm state does not exist".into()))?;
        if !state
            .agents
            .iter()
            .any(|agent| agent.id == assigned_agent_id)
        {
            return Err(SwarmError::AgentNotFound(assigned_agent_id));
        }
        let timestamp = now_ms();
        state.tasks.push(SwarmTask {
            id: generate_id("task"),
            title,
            description,
            assigned_agent_id,
            owned_files: Vec::new(),
            status: "queued".to_string(),
            depends_on: Vec::new(),
            created_at: timestamp,
            completed_at: None,
            last_updated_at: timestamp,
        });
        state.revision = state.revision.saturating_add(1);
        self.write_state_unlocked(dir, &state).await?;
        drop(_guard);
        self.emit_state(&state);
        Ok(state)
    }

    pub async fn update_task(
        &self,
        dir: &str,
        task_id: &str,
        status: &str,
    ) -> Result<SwarmState, SwarmError> {
        validate_identifier(task_id)?;
        if !matches!(
            status,
            "queued" | "building" | "review" | "done" | "blocked" | "stalled"
        ) {
            return Err(SwarmError::InvalidIdentifier(status.to_string()));
        }
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        let mut state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::TaskNotFound(task_id.to_string()))?;
        let task = state
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| SwarmError::TaskNotFound(task_id.to_string()))?;
        task.status = status.to_string();
        task.last_updated_at = now_ms();
        task.completed_at = (status == "done").then_some(task.last_updated_at);
        state.revision = state.revision.saturating_add(1);
        self.write_state_unlocked(dir, &state).await?;
        drop(_guard);
        self.emit_state(&state);
        Ok(state)
    }

    pub async fn send_message(
        &self,
        dir: &str,
        from: &str,
        to: &str,
        msg: &str,
    ) -> Result<(), SwarmError> {
        validate_identifier(from)?;
        validate_identifier(to)?;
        validate_content(msg)?;
        let lock = self.state_lock(dir)?;
        let _guard = lock.lock().await;
        let state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::InvalidIdentifier("swarm state does not exist".into()))?;
        if !state.agents.iter().any(|agent| agent.id == from) {
            return Err(SwarmError::AgentNotFound(from.to_string()));
        }
        if !state.agents.iter().any(|agent| agent.id == to) {
            return Err(SwarmError::AgentNotFound(to.to_string()));
        }
        let mailbox_dir = PathBuf::from(dir).join(".ade").join("mailbox");
        let mailbox_path = mailbox_dir.join(format!("{}.json", to));
        let lock_path = mailbox_dir.join(format!("{}.lock", to));
        let tmp_path = mailbox_dir.join(format!("{}.json.tmp.{}", to, uuid::Uuid::new_v4()));
        let from = from.to_string();
        let to = to.to_string();
        let msg = msg.to_string();

        let message = tokio::task::spawn_blocking(move || -> Result<MailboxMessage, SwarmError> {
            std::fs::create_dir_all(&mailbox_dir)?;
            let lock_file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)?;
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&lock_file);
            if unsafe { libc::flock(fd, libc::LOCK_EX) } != 0 {
                return Err(SwarmError::Io(std::io::Error::last_os_error()));
            }
            let mut messages: Vec<MailboxMessage> = match std::fs::read_to_string(&mailbox_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|error| {
                    let sidecar = format!("{}.corrupt", mailbox_path.display());
                    log::warn!("mailbox parse failed ({error}); quarantining to {sidecar}");
                    let _ = std::fs::rename(&mailbox_path, sidecar);
                    Vec::new()
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(SwarmError::Io(error)),
            };
            let message = MailboxMessage {
                id: generate_id("msg"),
                from,
                to,
                content: msg,
                timestamp: now_ms(),
                read: false,
            };
            messages.push(message.clone());
            let content = serde_json::to_string_pretty(&messages)?;
            let mut file = std::fs::File::create(&tmp_path)?;
            std::io::Write::write_all(&mut file, content.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(tmp_path, mailbox_path)?;
            Ok(message)
        })
        .await
        .map_err(|error| SwarmError::Io(std::io::Error::other(error)))??;

        // Keep the canonical mission feed in sync with the mailbox file. The
        // mailbox remains the delivery primitive; the state copy powers the
        // board/activity stream and is updated under the same per-workspace
        // lock as all other mission mutations.
        let mut state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::InvalidIdentifier("swarm state disappeared".into()))?;
        state.messages.push(message);
        state.revision = state.revision.saturating_add(1);
        self.write_state_unlocked(dir, &state).await?;
        drop(_guard);
        self.emit_state(&state);
        Ok(())
    }

    pub async fn read_mailbox(
        &self,
        dir: &str,
        agent_id: &str,
    ) -> Result<Vec<MailboxMessage>, SwarmError> {
        validate_identifier(agent_id)?;
        let state = self
            .read_state_unlocked(dir)
            .await?
            .ok_or_else(|| SwarmError::InvalidIdentifier("swarm state does not exist".into()))?;
        if !state.agents.iter().any(|agent| agent.id == agent_id) {
            return Err(SwarmError::AgentNotFound(agent_id.to_string()));
        }
        let mailbox_path = PathBuf::from(dir)
            .join(".ade")
            .join("mailbox")
            .join(format!("{}.json", agent_id));
        match fs::read_to_string(mailbox_path.clone()).await {
            Ok(content) => Ok(serde_json::from_str(&content)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(SwarmError::Io(error)),
        }
    }

    fn watch_is_current(&self, dir: &str, generation: u64) -> bool {
        self.watch_generations
            .lock()
            .map(|generations| generations.get(dir).copied() == Some(generation))
            .unwrap_or(false)
    }

    pub async fn watch_state(&self, dir: &str) -> Result<(), SwarmError> {
        let token = tokio_util::sync::CancellationToken::new();
        let generation = {
            let mut generations = self
                .watch_generations
                .lock()
                .map_err(|_| SwarmError::LockPoisoned)?;
            if generations.contains_key(dir) {
                return Ok(());
            }
            // A process-wide monotonic generation prevents a canceled watcher
            // from ever colliding with a later watcher after stop/restart.
            let next = self
                .next_watch_generation
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            generations.insert(dir.to_string(), next);
            next
        };
        {
            let mut tokens = self
                .cancel_tokens
                .lock()
                .map_err(|_| SwarmError::LockPoisoned)?;
            tokens.insert(dir.to_string(), token.clone());
        }
        let coordinator = self.clone();
        let dir_owned = dir.to_string();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
                    _ = token.cancelled() => break,
                }
                if token.is_cancelled() || !coordinator.watch_is_current(&dir_owned, generation) {
                    break;
                }
                let lock = match coordinator.state_lock(&dir_owned) {
                    Ok(lock) => lock,
                    Err(_) => break,
                };
                let _guard = lock.lock().await;
                if token.is_cancelled() || !coordinator.watch_is_current(&dir_owned, generation) {
                    break;
                }
                let Some(mut state) = (match coordinator.read_state_unlocked(&dir_owned).await {
                    Ok(state) => state,
                    Err(error) => {
                        log::warn!("swarm watcher read failed: {error}");
                        continue;
                    }
                }) else {
                    continue;
                };
                let now = now_ms();
                let mut modified = false;
                for agent in &mut state.agents {
                    if agent.status != "done"
                        && agent.status != "stalled"
                        && agent.last_action_at > 0
                        && now.saturating_sub(agent.last_action_at) as u64 > STALLED_TIMEOUT_MS
                    {
                        agent.status = "stalled".to_string();
                        agent.last_action = "No activity detected".to_string();
                        modified = true;
                    }
                }
                if modified {
                    if token.is_cancelled() || !coordinator.watch_is_current(&dir_owned, generation)
                    {
                        break;
                    }
                    state.revision = state.revision.saturating_add(1);
                    if let Err(error) = coordinator.write_state_unlocked(&dir_owned, &state).await {
                        log::warn!("swarm watcher write failed: {error}");
                        continue;
                    }
                }
                drop(_guard);
                if token.is_cancelled() || !coordinator.watch_is_current(&dir_owned, generation) {
                    break;
                }
                if let Some(ref tx) = coordinator.watch_tx {
                    let _ = tx.send(state.clone());
                }
                coordinator.emit_state(&state);
            }
            if let Ok(mut generations) = coordinator.watch_generations.lock() {
                if generations.get(&dir_owned).copied() == Some(generation) {
                    generations.remove(&dir_owned);
                }
            }
        });
        Ok(())
    }

    pub fn stop_watch(&self, dir: &str) -> Result<(), SwarmError> {
        let mut tokens = self
            .cancel_tokens
            .lock()
            .map_err(|_| SwarmError::LockPoisoned)?;
        if let Some(token) = tokens.remove(dir) {
            token.cancel();
        }
        // Remove the registration after cancelling. A new watcher cannot be
        // admitted until this generation entry is gone, and an old cleanup
        // only removes its own matching generation.
        if let Ok(mut generations) = self.watch_generations.lock() {
            generations.remove(dir);
        }
        Ok(())
    }

    pub fn subscribe(&self) -> watch::Receiver<SwarmState> {
        self.watch_rx.clone()
    }
}

impl Drop for SwarmCoordinator {
    fn drop(&mut self) {
        if let Ok(mut tokens) = self.cancel_tokens.lock() {
            for (_, token) in tokens.drain() {
                token.cancel();
            }
        }
        if let Ok(mut generations) = self.watch_generations.lock() {
            generations.clear();
        }
    }
}
