//! Windows PTY backend: ConPTY (pseudoconsole API).
//!
//! Implements the same surface as the Unix backend (`unix.rs`). The full
//! ConPTY wiring (CreatePseudoConsole + attribute-list CreateProcess) lands
//! behind this surface; every entry point currently returns
//! `ErrorKind::Unsupported` so the session layer degrades cleanly instead of
//! panicking, and the app can ship Linux first while Windows QA proceeds.
//!
//! Deliberate degradation notes (see ROADMAP / port plan):
//! - Shell-integration startup files (zsh/bash/fish rc injection) do not
//!   exist on Windows; `new_session` rejects spawns until ConPTY lands.
//! - `raw_master_fd` returns None: there is no numeric fd to tcgetpgrp, so
//!   agent foreground detection falls back to "shell".

use super::{SpawnSpec, Spawned};
use std::io;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;

/// Result of the non-blocking write fast path. Mirrors the Unix enum.
pub enum WriteFast {
    /// Fully written inline.
    Done(usize),
    /// Partially written; `remaining` must be re-driven on the blocking path.
    Partial { written: usize, remaining: Vec<u8> },
    /// Pipe full — retry the whole buffer on the blocking path.
    WouldBlock,
}

/// Reap a child process. ConPTY has no signal groups, so the real
/// implementation terminates the process handle stored in `ProcessHandle`
/// (TerminateProcess + grace + forced close). The pid/pgid pair is accepted
/// for signature parity; on Windows they are equal and only used for
/// foreground-probe cache keys.
pub fn reap_child(_pid: i32, _pgid: i32) {
    // No-op until the ConPTY handle carries a child HANDLE; nothing to reap
    // because nothing can be spawned yet (new_session rejects).
}

/// Platform-owned ConPTY state. The sentinel + lifecycle-lock layout mirrors
/// the Unix fd-lease semantics: an in-flight syscall holds its own duplicated
/// handle, so closing cannot invalidate it mid-operation.
pub struct ProcessHandle {
    /// ConPTY master pipe handle, or `-1` once closed.
    master: AtomicI32,
    /// Child process handle id, or `-1` once reaped.
    child: AtomicI32,
    /// Serializes handle duplication with closing. Same contract as the
    /// Unix fd_lifecycle_lock: never held across blocking I/O.
    lifecycle_lock: Mutex<()>,
}

impl ProcessHandle {
    pub fn new(master: i32) -> Self {
        Self {
            master: AtomicI32::new(master),
            child: AtomicI32::new(-1),
            lifecycle_lock: Mutex::new(()),
        }
    }

    /// Duplicate the live master handle (lease). No ConPTY wiring exists
    /// yet, so the lease route is unused on Windows; returning the
    /// closed-state error keeps the fd-lease contract without a panic a
    /// future caller could hit.
    pub fn duplicate_master_fd(&self) -> io::Result<Arc<ProcessHandle>> {
        let _guard = self.lifecycle_lock.lock();
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"))
    }

    /// Atomically close the master + child handles. Idempotent.
    pub fn close_fd_with_reason(&self, reason: &'static str) -> bool {
        let _guard = self.lifecycle_lock.lock();
        let m = self.master.swap(-1, Ordering::AcqRel);
        let c = self.child.swap(-1, Ordering::AcqRel);
        if m >= 0 || c >= 0 {
            log::debug!("ConPTY handles closed: reason={}", reason);
            true
        } else {
            false
        }
    }

    /// Write fast path parity: no ConPTY wiring yet, so surface the
    /// unsupported state through the same shape callers already handle.
    pub fn write_fast(&self, _data: &[u8]) -> io::Result<WriteFast> {
        Err(unsupported())
    }

    /// Blocking write path parity.
    pub fn write_slow(&self, _data: &[u8]) -> io::Result<usize> {
        Err(unsupported())
    }

    /// Read path parity (owned buffer; session layer copies back).
    pub fn read_bytes(&self, _buf: &mut [u8], _wait_ms: u64) -> io::Result<usize> {
        Err(unsupported())
    }

    /// Resize path parity.
    pub fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
        Err(unsupported())
    }

    /// No numeric master fd on Windows: foreground probing falls back to the
    /// spawn pgid in athena-core's detection layer.
    pub fn raw_master_fd(&self) -> Option<i32> {
        let fd = self.master.load(Ordering::Acquire);
        if fd >= 0 {
            Some(fd)
        } else {
            None
        }
    }
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows ConPTY backend is not wired yet; PTY sessions are unavailable on Windows builds",
    )
}

/// Spawn parity: rejects until the ConPTY implementation lands.
pub fn new_session(_spec: SpawnSpec<'_>) -> io::Result<Spawned> {
    Err(unsupported())
}

/// Reader-thread parity: delivers the unsupported error once, then closes.
pub fn spawn_reader(
    handle: &Arc<ProcessHandle>,
    _thread_name: String,
) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>(32);
    let err = match handle.master.load(Ordering::Acquire) {
        fd if fd >= 0 => unsupported(),
        _ => io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"),
    };
    let _ = tx.blocking_send(Err(err));
    rx
}

/// Best-effort deletion of startup files (parity; Windows never creates
/// them today, but the session layer calls this unconditionally on drop).
pub fn cleanup_startup_path(path: Option<&std::path::Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_dir_all(path);
        let _ = std::fs::remove_file(path);
    }
}

// Parity helpers re-exported through backend/mod.rs. On Windows these are
// meaningful only for Git-for-Windows POSIX shells; they return the same
// values as the Unix backend so shared tests hold shape.
pub fn shell_name(shell: &str) -> &str {
    shell.rsplit(['/', '\\']).next().unwrap_or(shell)
}

pub fn shell_flags(shell: &str) -> &'static [&'static str] {
    match shell_name(shell) {
        "zsh" => &["-f", "-i"],
        "bash" => &["--noprofile", "--norc", "-i"],
        "fish" => &["--no-config", "--interactive"],
        "sh" => &["-i"],
        _ => &[],
    }
}

pub fn startup_script_with_user_config(_shell: &str, integration_script: &str) -> String {
    // POSIX-rc sourcing is Unix-only; on Windows the integration script is
    // returned bare. Real consumers do not exist until ConPTY lands.
    integration_script.to_string()
}

pub fn child_environment_with_startup(
    startup_env: Option<(&str, &std::path::Path)>,
) -> Vec<std::ffi::CString> {
    let mut values = std::collections::BTreeMap::new();
    for (key, value) in std::env::vars() {
        values.insert(key, value);
    }
    values.insert("TERM".to_string(), "xterm-256color".to_string());
    values.insert("COLORTERM".to_string(), "truecolor".to_string());
    values.insert("TERM_PROGRAM".to_string(), "Athena".to_string());
    values.insert("KITTY_WINDOW_ID".to_string(), "athena".to_string());
    values.insert("ATHENA_SHELL_INTEGRATION".to_string(), "1".to_string());
    if let Some((key, path)) = startup_env {
        values.insert(key.to_string(), path.to_string_lossy().into_owned());
    }
    values
        .into_iter()
        .filter_map(|(key, value)| std::ffi::CString::new(format!("{key}={value}")).ok())
        .collect()
}
