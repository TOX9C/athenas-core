//! Swarm coordinator module — ported from electron/swarmCoordinator.ts
//!
//! Manages swarm state persistence, agent mailboxes, stalled agent detection,
//! and file watching. All file operations use atomic writes (write to tmp, then rename).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::EventEmitter;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during swarm operations.
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
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// An agent entry in the swarm state.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SwarmAgent {
    pub id: String,
    pub status: String,
    pub last_action_at: Option<u64>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// The top-level swarm state persisted to `.ade/swarm-state.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SwarmState {
    #[serde(default)]
    pub agents: Vec<SwarmAgent>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A message in an agent's mailbox.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: u64,
    pub read: bool,
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn generate_msg_id() -> String {
    format!("msg-{}", uuid::Uuid::new_v4())
}

/// Stalled agent timeout in milliseconds (90 seconds).
const STALLED_TIMEOUT_MS: u64 = 90_000;

// ---------------------------------------------------------------------------
// SwarmCoordinator
// ---------------------------------------------------------------------------

/// Thread-safe swarm coordinator that manages state persistence,
/// agent mailboxes, and stalled agent detection.
pub struct SwarmCoordinator {
    watch_tx: Option<watch::Sender<SwarmState>>,
    watch_rx: watch::Receiver<SwarmState>,
    cancel_tokens: Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    watching_dirs: Arc<Mutex<HashSet<String>>>,
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
            watching_dirs: self.watching_dirs.clone(),
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
            watching_dirs: Arc::new(Mutex::new(HashSet::new())),
            event_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    // -----------------------------------------------------------------------
    // State read / write (atomic)
    // -----------------------------------------------------------------------

    /// Read the swarm state from `.ade/swarm-state.json`.
    pub async fn read_state(&self, dir: &str) -> Result<Option<SwarmState>, SwarmError> {
        let state_path = PathBuf::from(dir).join(".ade").join("swarm-state.json");
        match fs::read_to_string(&state_path).await {
            Ok(content) => {
                let state: SwarmState = serde_json::from_str(&content)?;
                Ok(Some(state))
            }
            Err(_) => Ok(None),
        }
    }

    /// Write the swarm state to `.ade/swarm-state.json` using an atomic write.
    pub async fn write_state(&self, dir: &str, state: &SwarmState) -> Result<(), SwarmError> {
        let ade_dir = PathBuf::from(dir).join(".ade");
        fs::create_dir_all(&ade_dir).await?;

        let state_path = ade_dir.join("swarm-state.json");
        let tmp_path = ade_dir.join(format!("swarm-state.json.tmp.{}", now_ms()));

        let content = serde_json::to_string_pretty(state)?;
        {
            let mut file = fs::File::create(&tmp_path).await?;
            file.write_all(content.as_bytes()).await?;
            file.flush().await?;
        }

        fs::rename(&tmp_path, &state_path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Mailbox
    // -----------------------------------------------------------------------

    /// Send a message to an agent's mailbox (atomic write with lock).
    ///
    /// Uses `flock(2)` advisory locking on a per-agent lock file to serialize
    /// concurrent writers (which may be separate agent processes). Unlike the
    /// old create_new + stale-removal heuristic, `flock` is automatically
    /// released when the process exits — so a crashed writer cannot leave a
    /// stale lock, and we never risk stealing a legitimate slow writer's lock
    /// (which would cause silent message loss).
    pub async fn send_message(
        &self,
        dir: &str,
        from: &str,
        to: &str,
        msg: &str,
    ) -> Result<(), SwarmError> {
        let mailbox_dir = PathBuf::from(dir).join(".ade").join("mailbox");
        let mailbox_path = mailbox_dir.join(format!("{}.json", to));
        let lock_path = mailbox_dir.join(format!("{}.lock", to));
        let tmp_path = mailbox_dir.join(format!("{}.json.tmp.{}", to, now_ms()));
        // Clone to owned so the spawn_blocking closure can be 'static.
        let from = from.to_string();
        let to = to.to_string();
        let msg = msg.to_string();

        // The entire locked read-modify-write runs in spawn_blocking because
        // flock + std::fs are blocking. Holding the flock guard across the
        // blocking fs calls is exactly what serializes the writers.
        let result = tokio::task::spawn_blocking(move || -> Result<(), SwarmError> {
            std::fs::create_dir_all(&mailbox_dir)?;
            // Open or create the lock file, then acquire an exclusive flock.
            // LOCK_EX blocks until the lock is acquired — no retry loop, no
            // stale-lock heuristic. The lock auto-releases on handle drop /
            // process death.
            let lock_file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)?;
            // `as_raw_fd` + libc::flock. On any error we return and the
            // handle (and lock) drop, releasing the lock.
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&lock_file);
            let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if rc != 0 {
                return Err(SwarmError::Io(std::io::Error::last_os_error()));
            }
            // Lock held: do the read-modify-write. `lock_file` stays in scope
            // and is dropped (releasing the lock) at the end of this closure.
            let result = (|| -> Result<(), SwarmError> {
                let mut messages: Vec<MailboxMessage> = match std::fs::read_to_string(&mailbox_path)
                {
                    Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                    Err(_) => Vec::new(),
                };

                messages.push(MailboxMessage {
                    id: generate_msg_id(),
                    from: from.to_string(),
                    to: to.to_string(),
                    content: msg.to_string(),
                    timestamp: now_ms(),
                    read: false,
                });

                let content = serde_json::to_string_pretty(&messages)?;
                {
                    let mut file = std::fs::File::create(&tmp_path)?;
                    std::io::Write::write_all(&mut file, content.as_bytes())?;
                    std::io::Write::flush(&mut file)?;
                }
                std::fs::rename(&tmp_path, &mailbox_path)?;
                Ok(())
            })();
            // Drop the lock file handle → flock released. (Implicit at end of
            // closure, but explicit comment documents the invariant.)
            result
        })
        .await
        .map_err(|e| SwarmError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

        Ok(result)
    }

    /// Read the mailbox for a given agent.
    pub async fn read_mailbox(
        &self,
        dir: &str,
        agent_id: &str,
    ) -> Result<Vec<MailboxMessage>, SwarmError> {
        let mailbox_path = PathBuf::from(dir)
            .join(".ade")
            .join("mailbox")
            .join(format!("{}.json", agent_id));

        match fs::read_to_string(&mailbox_path).await {
            Ok(content) => Ok(serde_json::from_str(&content).unwrap_or_default()),
            Err(_) => Ok(Vec::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Stalled agent detection / state watching
    // -----------------------------------------------------------------------

    /// Start watching the swarm state file for changes.
    ///
    /// Every 5 seconds, reads the state file, detects stalled agents
    /// (those with `lastActionAt` older than 90 seconds whose status is
    /// neither "done" nor "stalled"), updates their status, and notifies
    /// via the watch channel.
    ///
    /// If the directory is already being watched, this returns immediately.
    pub async fn watch_state(&self, dir: &str) -> Result<(), SwarmError> {
        {
            let mut dirs = self
                .watching_dirs
                .lock()
                .map_err(|_| SwarmError::LockPoisoned)?;
            if dirs.contains(dir) {
                return Ok(());
            }
            dirs.insert(dir.to_string());
        }

        let token = tokio_util::sync::CancellationToken::new();
        {
            let mut tokens = self
                .cancel_tokens
                .lock()
                .map_err(|_| SwarmError::LockPoisoned)?;
            tokens.insert(dir.to_string(), token.clone());
        }

        let dir_owned = dir.to_string();
        let watch_tx = self.watch_tx.clone();
        let event_emitter = self.event_emitter.clone();
        let watching_dirs = self.watching_dirs.clone();
        let cancel_tokens = self.cancel_tokens.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
                    _ = token.cancelled() => break,
                }

                let state_path = PathBuf::from(&dir_owned)
                    .join(".ade")
                    .join("swarm-state.json");

                let content = match fs::read_to_string(&state_path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mut state: SwarmState = match serde_json::from_str(&content) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let now = now_ms();
                let mut modified = false;

                for agent in &mut state.agents {
                    if agent.status != "done"
                        && agent.status != "stalled"
                        && agent
                            .last_action_at
                            .is_some_and(|la| now.saturating_sub(la) > STALLED_TIMEOUT_MS)
                    {
                        agent.status = "stalled".to_string();
                        modified = true;
                        log::warn!("Agent {} has stalled (no activity for 90s)", agent.id);
                    }
                }

                if modified {
                    let ade_dir = PathBuf::from(&dir_owned).join(".ade");
                    let tmp_path = ade_dir.join(format!("swarm-state.json.tmp.{}", now_ms()));
                    let state_path = ade_dir.join("swarm-state.json");

                    match serde_json::to_string_pretty(&state) {
                        Ok(serialized) => match fs::File::create(&tmp_path).await {
                            Ok(mut file) => {
                                if let Err(e) = file.write_all(serialized.as_bytes()).await {
                                    log::error!("swarm state write failed: {}", e);
                                }
                                if let Err(e) = file.flush().await {
                                    log::error!("swarm state flush failed: {}", e);
                                }
                                if let Err(e) = fs::rename(&tmp_path, &state_path).await {
                                    log::error!("swarm state rename failed: {}", e);
                                }
                            }
                            Err(e) => {
                                log::error!("swarm state file create failed: {}", e);
                            }
                        },
                        Err(e) => {
                            log::error!("swarm state serialization failed: {}", e);
                        }
                    }
                }

                if let Some(ref tx) = watch_tx {
                    let _ = tx.send(state.clone());
                }

                // Emit state change event
                if let Ok(guard) = event_emitter.lock() {
                    if let Some(ref emitter) = *guard {
                        emitter(
                            "swarm:stateChange",
                            &serde_json::json!({
                                "agentCount": state.agents.len(),
                                "agents": state.agents.iter().map(|a| serde_json::json!({
                                    "id": a.id,
                                    "status": a.status,
                                    "lastActionAt": a.last_action_at,
                                })).collect::<Vec<_>>(),
                            }),
                        );
                    }
                }
            }

            // Cleanup on task exit
            let mut dirs = watching_dirs.lock().unwrap_or_else(|e| e.into_inner());
            dirs.remove(&dir_owned);
            let mut tokens = cancel_tokens.lock().unwrap_or_else(|e| e.into_inner());
            tokens.remove(&dir_owned);
        });

        Ok(())
    }

    /// Stop watching the swarm state for a specific directory.
    pub fn stop_watch(&self, dir: &str) -> Result<(), SwarmError> {
        let mut tokens = self
            .cancel_tokens
            .lock()
            .map_err(|_| SwarmError::LockPoisoned)?;
        if let Some(token) = tokens.remove(dir) {
            token.cancel();
        }
        let mut dirs = self
            .watching_dirs
            .lock()
            .map_err(|_| SwarmError::LockPoisoned)?;
        dirs.remove(dir);
        Ok(())
    }

    /// Receive the latest swarm state via the watch channel.
    pub fn subscribe(&self) -> watch::Receiver<SwarmState> {
        self.watch_rx.clone()
    }
}

impl Drop for SwarmCoordinator {
    fn drop(&mut self) {
        if let Ok(mut tokens) = self.cancel_tokens.lock() {
            for (_dir, token) in tokens.drain() {
                token.cancel();
            }
        }
        if let Ok(mut dirs) = self.watching_dirs.lock() {
            dirs.clear();
        }
    }
}
