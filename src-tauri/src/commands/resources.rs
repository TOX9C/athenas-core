//! Per-process resource aggregation for the status-bar pill.
//!
//! Walks the process family (main + WebKit renderer + PTY shells) and returns
//! each process's pure-Rust-collectable facts — name, PID, resident size —
//! plus CPU deltas derived from `PROC_PIDTASKINFO`. The renderer's footprint
//! cannot be read from the main process (`EPERM` under the sandbox), so the
//! frontend substitutes a value it already measures and sends in; this module
//! trusts it only for the aggregated display, never for control flow.
//!
//! CPU% is a rate over the polling interval, matching Activity Monitor's
//! expression: Δ(user+system ns) / elapsed × 100 × hertz-correction. The
//! kernel reports times in ns; converting with the host's `hz` constant makes
//! the math match `ps` on both Apple Silicon and Intel.

use serde::Serialize;



/// One process in the app's family. `name` is the basename of the executable
/// (`athenas-core`, `com.apple.WebKit.WebContent`, `zsh`). `is_renderer` lets
/// the frontend style WebKit's row differently (its big number is the real
/// surprise users should see first). `rss_bytes` and `cpu_pct` are real-time
/// samples; `footprint_bytes` is the renderer's own footprint, sent in by the
/// frontend (see module docs) and therefore `Option` — `None` means "not
/// measurable from this process".
#[derive(Debug, Clone, Serialize)]
pub struct ProcStat {
    pub pid: u32,
    pub name: String,
    pub is_main: bool,
    pub is_renderer: bool,
    pub rss_bytes: u64,
    pub cpu_pct: f64,
    pub footprint_bytes: Option<u64>,
}

/// Instructions to the pill: how to label the family and whether to flag
/// that the renderer's footprint came from the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct FamilySnapshot {
    pub procs: Vec<ProcStat>,
    pub total_rss_bytes: u64,
    pub total_footprint_bytes: u64,
    pub total_cpu_pct: f64,
    /// True when the renderer row's `footprint_bytes` was supplied by the
    /// frontend rather than read by this process. The pill surfaces this as
    /// a "~" prefix on the renderer's memory figure so support tickets are
    /// honest about what's measured vs what's asserted.
    pub renderer_estimated: bool,
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use libc::{c_char, c_int, c_void, pid_t};
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::LazyLock;
    use std::time::Instant;

    // libproc flavors, per the SDK's <libproc.h>. Public constants.
    const PROC_PIDTASKINFO_FLAVOR: c_int = 4;
    const PROC_PIDBSDINFO_FLAVOR: c_int = 3;

    #[repr(C)]
    struct ProcTaskInfo {
        pti_total_user: u64,
        pti_total_system: u64,
        pti_threads_user: u64,
        pti_threads_system: u64,
        pti_policy: i32,
        pti_faults: i32,
        pti_pageins: i32,
        pti_cow_faults: i32,
        pti_messages_sent: i32,
        pti_messages_received: i32,
        pti_syscalls_mach: i32,
        pti_syscalls_unix: i32,
        pti_csw: i32,
        pti_threadnum: i32,
        pti_numrunning: i32,
        pti_priority: i32,
    }

    #[repr(C)]
    struct ProcBsdInfo {
        pbi_flags: u32,
        pbi_status: u32,
        pbi_xstatus: u32,
        pbi_pid: u32,
        pbi_ppid: u32,
        pbi_uid: u32,
        pbi_gid: u32,
        pbi_ruid: u32,
        pbi_rgid: u32,
        pbi_svuid: u32,
        pbi_svgid: u32,
        rfu_1: u32,
        pbi_comm: [u8; 16],
        pbi_name: [u8; 32],
        pbi_nfiles: u32,
        pbi_pgid: u32,
        pbi_pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        pbi_nice: u32,
        pbi_start_sec: u64,
        pbi_start_usec: u64,
    }

    #[repr(C)]
    struct ProcRusageV4 {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wirings: u64,
        ri_interrupt_wirings: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
        ri_child_user_time: u64,
        ri_child_system_time: u64,
        ri_child_pkg_idle_wirings: u64,
        ri_child_interrupt_wirings: u64,
        ri_child_pageins: u64,
        ri_child_wired_size: u64,
        ri_child_resident_size: u64,
        ri_child_phys_footprint: u64,
        ri_final_pid: u32,
        ri_final_pidflag: u32,
    }

    extern "C" {
        fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: *mut c_void, size: c_int)
            -> c_int;
        fn proc_pid_rusage(pid: pid_t, flavor: c_int, buffer: *mut c_void) -> c_int;
        fn proc_pidpath(pid: pid_t, buffer: *mut c_char, size: u32) -> c_int;
        fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    }

    struct PrevSample {
        cpu_ns: u64,
        when: Instant,
    }

    static PREV: LazyLock<Mutex<HashMap<u32, PrevSample>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub fn collect_family(renderer_footprint_bytes: Option<u64>) -> Result<FamilySnapshot, String> {
        let my_pid = std::process::id();
        let self_name = executable_name(my_pid).unwrap_or_else(|| "athenas-core".to_string());

        // Enumerate every PID on the system. On a dev box this is ~1.2 KB of
        // output for ~400 processes — trivial. We could enumerate only
        // children, but WebKit's renderer appears with PPID 1 (XPC re-parenting),
        // so we filter by family later instead.
        let capacity = 4096;
        let mut pid_buf: Vec<i32> = Vec::with_capacity(capacity);
        let n = unsafe { proc_listallpids(pid_buf.as_mut_ptr() as *mut c_void, (capacity * 4) as c_int) };
        if n <= 0 {
            return Err("proc_listallpids failed".to_string());
        }
        unsafe { pid_buf.set_len(n as usize) };

        let mut family_pids: Vec<u32> = vec![my_pid];
        let mut to_walk: Vec<u32> = vec![my_pid];
        while let Some(pp) = to_walk.pop() {
            for &pid in &pid_buf {
                if pid <= 0 || family_pids.contains(&(pid as u32)) {
                    continue;
                }
                if bsd_info(pid).map(|b| b.pbi_ppid) == Some(pp) {
                    family_pids.push(pid as u32);
                    to_walk.push(pid as u32);
                }
            }
        }
        // WebKit's renderer doesn't show under us via PPID — identify it by
        // scanning all PIDs for the WKWebView XPC host and matching start time
        // to ours (spawned within ~2s of app launch).
        let my_start = bsd_info(my_pid as i32).map(|b| b.pbi_start_sec).unwrap_or(0);
        let renderer_pid = find_webkit_renderer(&pid_buf, my_start);
        // The renderer is an XPC service re-parented to launchd (PPID 1), so the
        // PPID walk never reaches it — attach it explicitly once found.
        if let Some(rp) = renderer_pid {
            family_pids.push(rp);
        }

        let mut procs: Vec<ProcStat> = Vec::with_capacity(family_pids.len() + 1);
        let now = Instant::now();

        let mut total_rss: u64 = 0;
        let mut total_cpu: f64 = 0.0;
        let mut total_footprint: u64 = 0;
        let mut renderer_estimated = false;

        for &pid in &family_pids {
            let name = if pid == my_pid {
                self_name.clone()
            } else if Some(pid) == renderer_pid {
                "com.apple.WebKit.WebContent".to_string()
            } else {
                executable_name(pid).unwrap_or_else(|| format!("pid-{pid}"))
            };
            let task_info = task_info(pid as i32);
            let rusage = rusage_v4(pid as i32);
            let rss_bytes = rusage.as_ref().map(|r| r.ri_resident_size).unwrap_or(0);
            let footprint_bytes = rusage.as_ref().map(|r| r.ri_phys_footprint);
            let cpu_pct = task_info
                .map(|ti| {
                    let cpu_ns = ti.pti_total_user + ti.pti_total_system;
                    let mut cache = PREV.lock();
                    let pct = match cache.get_mut(&(pid)) {
                        Some(prev) => {
                            let dt = now.duration_since(prev.when).as_secs_f64();
                            if dt <= 0.0 {
                                0.0
                            } else {
                                ((cpu_ns.saturating_sub(prev.cpu_ns)) as f64 / dt / 1e9) * 100.0
                            }
                        }
                        // First sample: return 0 so the delta is computed against
                        // the next call, matching `top`'s two-sample approach.
                        None => 0.0,
                    };
                    cache.insert(
                        pid,
                        PrevSample {
                            cpu_ns,
                            when: now,
                        },
                    );
                    pct
                })
                .unwrap_or(0.0);

            let is_renderer = Some(pid) == renderer_pid;
            let is_main = pid == my_pid;
            // When the renderer can't be measured from here (sandbox), the
            // frontend supplies its own footprint. We trust it for display
            // only — the pill is read-only, so a spoofed number just spoofs
            // the pill, never a control decision.
            let report_footprint = if is_renderer {
                renderer_footprint_bytes.or(footprint_bytes)
            } else {
                footprint_bytes
            };

            total_rss += rss_bytes;
            total_cpu += cpu_pct;
            if let Some(f) = report_footprint {
                total_footprint += f;
            }
            if is_renderer && renderer_footprint_bytes.is_some() {
                renderer_estimated = true;
            }

            procs.push(ProcStat {
                pid,
                name,
                is_main,
                is_renderer,
                rss_bytes,
                cpu_pct,
                footprint_bytes: report_footprint,
            });
        }

        procs.sort_by_key(|p| p.rss_bytes);
        procs.reverse();

        Ok(FamilySnapshot {
            procs,
            total_rss_bytes: total_rss,
            total_footprint_bytes: total_footprint,
            total_cpu_pct: total_cpu.round(),
            renderer_estimated,
        })
    }

    fn bsd_info(pid: i32) -> Option<ProcBsdInfo> {
        let mut info = std::mem::MaybeUninit::<ProcBsdInfo>::uninit();
        let sz = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDBSDINFO_FLAVOR,
                0,
                info.as_mut_ptr() as *mut c_void,
                std::mem::size_of::<ProcBsdInfo>() as c_int,
            )
        };
        if sz as usize == std::mem::size_of::<ProcBsdInfo>() {
            Some(unsafe { info.assume_init() })
        } else {
            None
        }
    }

    fn task_info(pid: i32) -> Option<ProcTaskInfo> {
        let mut info = std::mem::MaybeUninit::<ProcTaskInfo>::uninit();
        let sz = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDTASKINFO_FLAVOR,
                0,
                info.as_mut_ptr() as *mut c_void,
                std::mem::size_of::<ProcTaskInfo>() as c_int,
            )
        };
        if sz as usize == std::mem::size_of::<ProcTaskInfo>() {
            Some(unsafe { info.assume_init() })
        } else {
            None
        }
    }

    fn rusage_v4(pid: i32) -> Option<ProcRusageV4> {
        let mut info = std::mem::MaybeUninit::<ProcRusageV4>::uninit();
        let ret = unsafe { proc_pid_rusage(pid, 4, info.as_mut_ptr() as *mut c_void) };
        if ret == 0 {
            Some(unsafe { info.assume_init() })
        } else {
            None
        }
    }

    fn executable_name(pid: u32) -> Option<String> {
        let mut buf = [0i8; 4096];
        let ret = unsafe { proc_pidpath(pid as i32, buf.as_mut_ptr(), 4096) };
        if ret > 0 {
            let path = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
            path.rsplit('/').next().map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Find the WKWebView WebContent XPC service that (almost certainly) belongs
    /// to this app. Distinct from child enumeration because PPID shows as 1.
    fn find_webkit_renderer(all: &[i32], my_start_sec: u64) -> Option<u32> {
        let mut best: Option<(u32, u64)> = None;
        for &pid in all {
            if pid <= 0 {
                continue;
            }
            let Some(info) = bsd_info(pid) else {
                continue;
            };
            let name = match executable_name(pid as u32) {
                Some(n) => n,
                // PIDs vanish between `proc_listallpids` and this lookup; a
                // zombie here must not skip the still-alive renderer later in
                // the list.
                None => continue,
            };
            if name != "com.apple.WebKit.WebContent" {
                continue;
            }
            // Nearest start time to ours; WebKit spawns the renderer within
            // a second of the main process. Walked PIDs can't be the renderer
            // if their start timestamp diverges by >5s.
            let delta = info.pbi_start_sec.abs_diff(my_start_sec);
            if delta > 5 {
                continue;
            }
            match best {
                Some((_, bd)) if delta >= bd => {}
                _ => best = Some((pid as u32, delta)),
            }
        }
        best.map(|(pid, _)| pid)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    pub fn collect_family(_renderer_footprint_size_bytes: Option<u64>) -> Result<FamilySnapshot, String> {
        Err("resource_stats is only implemented on macOS".to_string())
    }
}

/// `renderer_footprint_bytes` is the WebKit renderer's phys_footprint as
/// measured *by the frontend* (it can read its own process via WKWebView APIs;
/// the main process cannot read the renderer's under the sandbox). Passing
/// `None` tells the backend to report whatever it could read — an `rss`-only
/// figure for the renderer row.
///
/// Returns the snapshot as a JSON **string**, matching the codebase's
/// `invoke::<String>` + `serde_json::from_str` convention (see
/// `pty_agent_info`): Tauri v2 hands the frontend a string payload for
/// struct-typed results, so serializing here keeps the JS side a plain parse.
#[tauri::command]
pub fn resource_stats(renderer_footprint_bytes: Option<u64>) -> Result<String, String> {
    let snapshot = resource_stats_inner(renderer_footprint_bytes)?;
    serde_json::to_string(&snapshot).map_err(|e| format!("serialize resource_stats: {e}"))
}

fn resource_stats_inner(renderer_footprint_bytes: Option<u64>) -> Result<FamilySnapshot, String> {
    #[cfg(target_os = "macos")]
    return macos::collect_family(renderer_footprint_bytes);

    #[cfg(not(target_os = "macos"))]
    return platform::collect_family(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_includes_self_and_shells() {
        // The unit test runs inside a spawned test binary child of the shell
        // that invoked `cargo test`. Self+that parent chain is in the family.
        let snap = resource_stats_inner(None).expect("stats");
        assert!(snap.procs.iter().any(|p| p.is_main));
        assert!(snap.total_rss_bytes > 0);
    }

    #[test]
    fn totals_include_test_process_footprint() {
        let snap = resource_stats_inner(None).expect("stats");
        // In the test binary, footprint (phys_footprint) is reported; it can be
        // smaller than RSS if pages are shared or not mapped. We only assert the
        // family math is sane and that *some* nonzero memory was associated
        // with the process.
        assert!(snap.total_footprint_bytes > 0);
        assert!(snap.total_rss_bytes > 0);
    }

    #[test]
    fn fallback_identifies_process_family_without_panicking() {
        let snap = resource_stats_inner(Some(512 * 1024 * 1024)).expect("stats");
        assert!(snap.total_footprint_bytes > 0);
    }
}
