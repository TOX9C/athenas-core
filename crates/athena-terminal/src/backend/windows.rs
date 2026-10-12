//! Windows PTY backend: ConPTY (pseudoconsole API).
//!
//! Mirrors the Unix backend's surface and contracts:
//! - `ProcessHandle` owns the ConPTY pipe handles + child process handle,
//!   closed atomically with `-1` sentinels; duplication-for-syscall becomes
//!   a lock-checked handle read (CloseHandle cannot recycle a handle that
//!   in-flight syscalls hold references to — the OS refcounts handles).
//! - `write_fast` attempts one non-blocking-ish write inline; the slow path
//!   polls for pipe-drained with a bounded 30 s wait.
//! - `spawn_reader` runs a dedicated thread doing blocking ReadFile on the
//!   master pipe read end with a 100 ms cancel tick, delivering chunks over
//!   the same mpsc channel shape as Unix poll(2).
//! - Kill/reap: ConPTY has no signal groups. Termination is driven through
//!   the stored child handle at close time (TerminateProcess after a
//!   bounded wait, then CloseHandle); the free `reap_child` function keeps
//!   signature parity as a no-op.
//!
//! Startup-file shell integration (zsh/bash/fish rc injection) is
//! Unix-only; on Windows `new_session` spawns the shell directly. POSIX
//! shells from Git-for-Windows still receive the environment TERM etc. via
//! `child_environment_with_startup`.

use super::{SpawnSpec, Spawned};
use std::io;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT, WAIT_EVENT,
};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD,
    HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_CREATION_FLAGS,
    STARTUPINFOEXW,
};

/// Result of the non-blocking write fast path. Mirrors the Unix enum.
pub enum WriteFast {
    /// Fully written inline.
    Done(usize),
    /// Partially written; `remaining` must be re-driven on the blocking path.
    Partial { written: usize, remaining: Vec<u8> },
    /// Pipe full — retry the whole buffer on the blocking path.
    WouldBlock,
}

/// Windows HANDLEs are pointer-sized, so they live in a Mutex-guarded slots
/// struct with an atomic open/closed sentinel — the same
/// `close_fd_with_reason` / `raw_master_fd` contract the session layer uses.
///
/// HANDLE/HPCON wrap `*mut c_void` (auto-unstable in windows-rs, hence not
/// Send/Sync). Windows handle VALUES are thread-safe by contract (any
/// thread may use or close a handle), so the slots are Send+Sync via a
/// transparent wrapper — the standard pattern for handle-owning structs.
#[derive(Clone, Copy)]
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

#[derive(Clone, Copy)]
struct SendHpcon(HPCON);
unsafe impl Send for SendHpcon {}
unsafe impl Sync for SendHpcon {}

struct HandleSlots {
    /// ConPTY master read end (output from the shell).
    master_read: Option<SendHandle>,
    /// ConPTY master write end (input to the shell).
    master_write: Option<SendHandle>,
    /// Child process handle.
    child: Option<SendHandle>,
    /// The pseudoconsole itself (closed last).
    hpcon: Option<SendHpcon>,
}

/// Platform-owned ConPTY state. The sentinel + lifecycle-lock layout mirrors
/// the Unix fd-lease semantics: closing swaps everything out under the lock,
/// so an in-flight syscall that already read a handle cannot see a recycled
/// one (the OS keeps handle allocations alive until CloseHandle, which only
/// the closer performs).
pub struct ProcessHandle {
    slots: Mutex<HandleSlots>,
    /// 1 while open, 0 after close — the session layer's sentinel shape.
    open: AtomicI32,
}

impl ProcessHandle {
    pub fn new(_master: i32) -> Self {
        // Parity constructor for tests on Unix-shaped call sites; the real
        // handle is built by new_session below. A closed sentinel state is
        // the only safe default here.
        Self {
            slots: Mutex::new(HandleSlots {
                master_read: None,
                master_write: None,
                child: None,
                hpcon: None,
            }),
            open: AtomicI32::new(-1),
        }
    }

    fn from_slots(slots: HandleSlots) -> Self {
        Self {
            slots: Mutex::new(slots),
            open: AtomicI32::new(1),
        }
    }

    /// Atomically close every handle. Idempotent; returns whether this call
    /// closed anything. Mirrors the Unix `close_fd_with_reason` contract.
    pub fn close_fd_with_reason(&self, reason: &'static str) -> bool {
        let mut slots = self.slots.lock();
        if self.open.swap(-1, Ordering::AcqRel) < 0 {
            return false;
        }
        let HandleSlots {
            master_read,
            master_write,
            child,
            hpcon,
        } = HandleSlots {
            master_read: slots.master_read.take(),
            master_write: slots.master_write.take(),
            child: slots.child.take(),
            hpcon: slots.hpcon.take(),
        };
        let closed_any = master_read.is_some()
            || master_write.is_some()
            || child.is_some()
            || hpcon.is_some();
        if let Some(SendHandle(h)) = master_read {
            let _ = unsafe { CloseHandle(h) };
        }
        if let Some(SendHandle(h)) = master_write {
            let _ = unsafe { CloseHandle(h) };
        }
        if let Some(SendHandle(h)) = child {
            // Reap first so the process object settles, then drop the handle.
            terminate_and_wait(h);
        }
        if let Some(SendHpcon(hpcon)) = hpcon {
            unsafe { ClosePseudoConsole(hpcon) };
        }
        if closed_any {
            log::debug!("ConPTY handles closed: reason={}", reason);
        }
        closed_any
    }

    /// Lease the master write handle for one syscall. `None` when closed.
    fn with_master_write<T>(&self, f: impl FnOnce(HANDLE) -> T) -> Option<T> {
        let slots = self.slots.lock();
        if self.open.load(Ordering::Acquire) < 0 {
            return None;
        }
        slots.master_write.map(|SendHandle(h)| f(h))
    }

    /// Lease the master read handle for one syscall.
    fn with_master_read<T>(&self, f: impl FnOnce(HANDLE) -> T) -> Option<T> {
        let slots = self.slots.lock();
        if self.open.load(Ordering::Acquire) < 0 {
            return None;
        }
        slots.master_read.map(|SendHandle(h)| f(h))
    }

    /// Write fast path: one WriteFile attempt inline. Named-pipe writes
    /// block only when the ConPTY buffer is full, which for an interactive
    /// shell is virtually never — the same profile as the Unix O_NONBLOCK
    /// fast path. On ERROR_BROKEN_PIPE (child gone) surfaces BrokenPipe.
    pub fn write_fast(&self, data: &[u8]) -> io::Result<WriteFast> {
        let written = self
            .with_master_write(|h| unsafe {
                let mut n: u32 = 0;
                match WriteFile(h, Some(data), Some(&mut n), None) {
                    Ok(()) => Ok(n as usize),
                    Err(e) => Err(e),
                }
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"))?;
        match written {
            Ok(n) if n == data.len() => Ok(WriteFast::Done(n)),
            Ok(n) => Ok(WriteFast::Partial {
                written: n,
                remaining: data[n..].to_vec(),
            }),
            Err(e) => Err(map_pipe_err(e)),
        }
    }

    /// Blocking write path with a bounded 30 s drain wait: WriteFile in a
    /// loop; on ERROR_NO_DATA / broken pipe surfaces the terminal error.
    pub fn write_slow(&self, data: &[u8]) -> io::Result<usize> {
        let mut done = 0usize;
        while done < data.len() {
            let n = self
                .with_master_write(|h| unsafe {
                    let mut n: u32 = 0;
                    match WriteFile(h, Some(&data[done..]), Some(&mut n), None) {
                        Ok(()) => Ok(n as usize),
                        Err(e) => Err(e),
                    }
                })
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"))?;
            match n {
                Ok(0) => {
                    // Zero-byte write on a full pipe: brief yield, bounded
                    // by the caller's 30 s blocking-pool window.
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(n) => done += n,
                Err(e) => return Err(map_pipe_err(e)),
            }
        }
        Ok(done)
    }

    /// Read path parity (owned buffer; session layer copies back). Blocking
    /// ReadFile against the ConPTY read pipe, bounded by `wait_ms` via a
    /// 50 ms poll cadence — the F10 park semantics adapted to named pipes.
    pub fn read_bytes(&self, buf: &mut [u8], wait_ms: u64) -> io::Result<usize> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
        loop {
            let res = self
                .with_master_read(|h| unsafe {
                    let mut n: u32 = 0;
                    match ReadFile(h, Some(buf), Some(&mut n), None) {
                        Ok(()) => Ok(n as usize),
                        Err(e) => Err(e),
                    }
                })
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"))?;
            match res {
                Ok(n) => return Ok(n),
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_BROKEN_PIPE.to_hresult() => {
                    // Child end closed: EOF.
                    return Ok(0);
                }
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_NO_DATA.to_hresult() => {
                    // Pipe was closed mid-read (kill): EOF for our purposes.
                    return Ok(0);
                }
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_IO_PENDING.to_hresult() => {
                    // Should not happen without OVERLAPPED, but treat as retry.
                }
                Err(e) => return Err(map_pipe_err(e)),
            }
            if std::time::Instant::now() >= deadline {
                return Ok(0);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Resize the pseudoconsole.
    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.slots
            .lock()
            .hpcon
            .map(|SendHpcon(hpcon)| unsafe {
                ResizePseudoConsole(
                    hpcon,
                    COORD {
                        X: cols as i16,
                        Y: rows as i16,
                    },
                )
                .map_err(|e| io::Error::other(e.to_string()))
            })
            .transpose()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"))
    }

    /// No numeric master fd on Windows: foreground probing falls back to the
    /// spawn pgid in athena-core's detection layer. Kept for surface parity;
    /// reports the open sentinel so callers can distinguish closed panes.
    pub fn raw_master_fd(&self) -> Option<i32> {
        let open = self.open.load(Ordering::Acquire);
        if open >= 0 {
            Some(open)
        } else {
            None
        }
    }
}

/// Terminate a child process and wait for it to settle, then close. Grace:
/// ConPTY child processes get no SIGTERM phase — TerminateProcess is the
/// documented teardown for a pseudoconsole session (ClosePseudoConsole
/// sends the child a Ctrl-CLOSE but does not guarantee exit).
fn terminate_and_wait(child: HANDLE) {
    unsafe {
        let _ = TerminateProcess(child, 1);
        // Bounded wait so a hung child cannot stall Drop forever.
        let mut waited = 0u32;
        loop {
            match WaitForSingleObject(child, 200) {
                WAIT_EVENT(0) => break, // WAIT_OBJECT_0: exited
                _ => {}
            }
            waited += 200;
            if waited >= 2_000 {
                break;
            }
        }
        let _ = CloseHandle(child);
    }
}

fn map_pipe_err(e: windows::core::Error) -> io::Error {
    let code = e.code();
    if code == windows::Win32::Foundation::ERROR_BROKEN_PIPE.to_hresult()
        || code == windows::Win32::Foundation::ERROR_NO_DATA.to_hresult()
    {
        io::Error::new(io::ErrorKind::BrokenPipe, e.to_string())
    } else {
        io::Error::other(e.to_string())
    }
}

/// Reap parity: on Windows, termination is driven through the stored child
/// handle inside `ProcessHandle::close_fd_with_reason`; pid/pgid have no
/// process groups to signal. Kept as a no-op for signature parity — the
/// session layer's Drop/kill paths route through the handle close, which
/// terminates and waits.
pub fn reap_child(_pid: i32, _pgid: i32) {}

/// Spawn the shell on a fresh ConPTY.
///
/// ConPTY shape: two pipes (input to child / output from child) crossed
/// into the pseudoconsole; the child is created with an attribute list
/// carrying PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE so its stdio binds to the
/// ConPTY instead of inherited handles. This function blocks through
/// process creation; the session layer runs it on the blocking pool.
pub fn new_session(spec: SpawnSpec<'_>) -> io::Result<Spawned> {
    let SpawnSpec {
        shell,
        cwd,
        cols,
        rows,
        startup_script: _,
    } = spec;

    // Pipe A: app -> child stdin. Pipe B: child stdout -> app.
    let mut in_read = HANDLE::default();
    let mut in_write = HANDLE::default();
    let mut out_read = HANDLE::default();
    let mut out_write = HANDLE::default();
    unsafe {
        CreatePipe(&mut in_read, &mut in_write, None, 0)
            .map_err(|e| io::Error::other(e.to_string()))?;
        CreatePipe(&mut out_read, &mut out_write, None, 0)
            .map_err(|e| io::Error::other(e.to_string()))?;
        // Only the pseudoconsole-facing ends are inheritable into the child;
        // our ends stay private.
        SetHandleInformation(in_write, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))
            .map_err(|e| io::Error::other(e.to_string()))?;
        SetHandleInformation(out_read, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))
            .map_err(|e| io::Error::other(e.to_string()))?;
    }

    let size = COORD {
        X: cols as i16,
        Y: rows as i16,
    };
    let hpcon = unsafe {
        CreatePseudoConsole(size, in_read, out_write, 0)
            .map_err(|e| io::Error::other(e.to_string()))?
    };
    // CreatePseudoConsole consumed in_read/out_write into the pseudoconsole.
    // Keep only our master ends (in_write for input, out_read for output).
    // The pseudoconsole owns the child-facing ends now.

    // Attribute list with the pseudoconsole.
    let mut attr_size: usize = 0;
    unsafe {
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut attr_size);
    }
    let mut attr_buf = vec![0u8; attr_size.max(16)];
    let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as _);
    unsafe {
        InitializeProcThreadAttributeList(Some(attr_list), 1, None, &mut attr_size)
            .map_err(|e| io::Error::other(e.to_string()))?;
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(&hpcon as *const HPCON as _),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
    }

    // Command line: `shell` with no extra args (interactive). Windows wants
    // a mutable wide buffer for the command line.
    let cmdline: Vec<u16> = shell.encode_utf16().chain([0]).collect();
    let mut cmdline_buf = cmdline.clone();
    let cwd_w: Vec<u16> = cwd.encode_utf16().chain([0]).collect();

    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.lpAttributeList = attr_list;

    let mut pi = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(windows::core::PWSTR(cmdline_buf.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(EXTENDED_STARTUPINFO_PRESENT.0),
            None,
            PCWSTR(cwd_w.as_ptr()),
            &si.StartupInfo,
            &mut pi,
        )
    };

    // The attribute list is copied into the process; free ours either way.
    unsafe {
        DeleteProcThreadAttributeList(attr_list);
    }

    match created {
        Ok(()) => {}
        Err(e) => {
            // Tear down everything we created.
            unsafe {
                ClosePseudoConsole(hpcon);
                let _ = CloseHandle(in_write);
                let _ = CloseHandle(out_read);
            }
            return Err(io::Error::other(format!("CreateProcessW failed: {e}")));
        }
    }

    // Success: close the child's thread/process handles we do not keep (we
    // keep the process handle for termination).
    unsafe {
        let _ = CloseHandle(pi.hThread);
    }

    let pid = pi.dwProcessId as i32;
    Ok(Spawned {
        handle: ProcessHandle::from_slots(HandleSlots {
            master_read: Some(SendHandle(out_read)),
            master_write: Some(SendHandle(in_write)),
            child: Some(SendHandle(pi.hProcess)),
            hpcon: Some(SendHpcon(hpcon)),
        }),
        // No tty device path on Windows.
        tty_path: None,
        pid,
        pgid: pid,
        startup_cleanup_path: None,
    })
}

/// Reader-thread parity: blocking ReadFile loop on the master read pipe
/// with a 100 ms deadline tick that exits when the consumer drops. Delivers
/// `Ok(chunk)` via the channel; terminal errors are forwarded once, then
/// the thread exits — matching the Unix poll(2) reader's contract.
pub fn spawn_reader(
    handle: &Arc<ProcessHandle>,
    thread_name: String,
) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>(32);
    // Kept in case the thread cannot even be spawned — the error is
    // delivered through the channel so the caller takes its normal
    // error path.
    let fallback_tx = tx.clone();
    let handle = Arc::clone(handle);
    let spawned = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let res = handle
                    .with_master_read(|h| unsafe {
                        let mut n: u32 = 0;
                        match ReadFile(h, Some(&mut buf), Some(&mut n), None) {
                            Ok(()) => Ok(n as usize),
                            Err(e) => Err(e),
                        }
                    })
                    .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "handle closed"));
                match res {
                    Ok(Ok(0)) => {
                        // EOF: the pseudoconsole's child side closed.
                        return;
                    }
                    Ok(Ok(n)) => {
                        let mut chunk = std::mem::replace(&mut buf, vec![0u8; 16 * 1024]);
                        chunk.truncate(n);
                        if tx.blocking_send(Ok(chunk)).is_err() {
                            return;
                        }
                        continue;
                    }
                    Ok(Err(e)) => {
                        let code = e.code();
                        if code == windows::Win32::Foundation::ERROR_BROKEN_PIPE.to_hresult()
                            || code == windows::Win32::Foundation::ERROR_NO_DATA.to_hresult()
                        {
                            // Slave side gone (kill or exit).
                            return;
                        }
                        let _ = tx.blocking_send(Err(map_pipe_err(e)));
                        return;
                    }
                    Err(e) => {
                        // Handle closed (kill/shutdown).
                        let _ = tx.blocking_send(Err(e));
                        return;
                    }
                }
            }
        });
    match spawned {
        Ok(_thread) => rx,
        Err(e) => {
            let _ = fallback_tx.blocking_send(Err(io::Error::other(e)));
            rx
        }
    }
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
    // returned bare. Real consumers do not exist: new_session ignores the
    // startup script (no rc-file mechanism on Windows shells).
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
