//! Platform PTY backend.
//!
//! The session layer (lifecycle, listener generations, grid, coalescing) is
//! platform-neutral; this module owns the OS surface each platform must
//! provide. Two static implementations exist — `unix` (openpty/fork/exec,
//! macOS + Linux) and `windows` (ConPTY) — selected at compile time via
//! `cfg`. No trait objects: the surface is fixed, both impls are static, and
//! monomorphized calls keep the per-keystroke paths call-free.
//!
//! Every handle is opaque to the session layer; the ONLY invariant the
//! session layer assumes is that `master` can be duplicated for the
//! duration of one syscall (fd-lease semantics) so a concurrent `close`
//! cannot recycle the descriptor under an in-flight operation.

#[cfg(unix)]
#[path = "unix.rs"]
mod imp;

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

pub use imp::{
    cleanup_startup_path, new_session, reap_child, spawn_reader, child_environment_with_startup,
    shell_flags, shell_name, startup_script_with_user_config, ProcessHandle, WriteFast,
};

/// Fixed spawn input the session layer passes to the platform backend.
pub struct SpawnSpec<'a> {
    pub shell: &'a str,
    pub cwd: &'a str,
    pub cols: u16,
    pub rows: u16,
    pub startup_script: Option<&'a str>,
}

/// Result of a successful backend spawn.
pub struct Spawned {
    /// Platform-owned master descriptor (fd on Unix, ConPTY handle on
    /// Windows). Stored inside `ProcessHandle`.
    pub handle: ProcessHandle,
    /// Best-effort slave TTY path for harness session association.
    pub tty_path: Option<String>,
    /// Opaque process id, used only for foreground-label probing.
    pub pid: i32,
    /// Opaque process-group id (== pid on Windows, where groups do not
    /// exist). Used for kill and foreground probing.
    pub pgid: i32,
    /// Private startup file/directory the backend created for shell
    /// integration; the session layer deletes it on drop.
    pub startup_cleanup_path: Option<std::path::PathBuf>,
}

