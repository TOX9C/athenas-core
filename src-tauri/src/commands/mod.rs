use crate::state::AppState;
use base64::Engine;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Minimal HTML escape to prevent XSS in contexts that might render HTML.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub mod caps;

// ── Path validation helpers ──────────────────────────────────────────────────

/// Get the canonicalized workspace root for path sandboxing.
///
/// The workspace root is the *project* directory — the ancestor that contains
/// `src-tauri/` — not the process's current working directory, which is
/// launch-context-dependent:
///   - `cargo tauri dev` runs the backend with cwd = `src-tauri/`.
///   - A bundled release `.app` launched from Finder has cwd = `/`.
/// Using `current_dir()` directly made the sandbox root `src-tauri/` in dev
/// (so the real project root one level up was wrongly rejected) and `/` in
/// release (so the sandbox silently allowed every path — a latent hole, not a
/// correct config).
///
/// Resolution: look for the project-root marker (`src-tauri/tauri.conf.json`)
/// by walking up from both `current_dir()` *and* the executable's directory.
/// The exe path is stable across launch contexts: in dev it is
/// `target/debug/athenas-core`, in release `…/Athena's Core.app/Contents/MacOS/…`,
/// both of which live under the project root when built locally. If neither
/// walk finds the marker, fall back to `current_dir()` so behavior is no worse
/// than before (and so the validator still has *some* root to check against).
fn get_workspace_root() -> Result<std::path::PathBuf, CommandError> {
    let raw = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("Failed to get workspace root: {}", e)))?;

    let exe = std::env::current_exe()
        .map_err(|e| CommandError::Internal(format!("Failed to get current exe: {}", e)))?;

    // Candidate starting points for the upward marker walk. `raw` (cwd)
    // wins in dev; `exe` wins for a Finder-launched release bundle whose
    // cwd is `/`. Both are cheap to try.
    let starts: Vec<std::path::PathBuf> = vec![raw.clone(), exe.clone()];

    let marker_name = std::path::Path::new("tauri.conf.json");
    let src_tauri = std::path::Path::new("src-tauri");

    let mut root_candidate: Option<std::path::PathBuf> = None;
    'outer: for start in &starts {
        let mut dir = start.as_path();
        loop {
            if dir.join(src_tauri).join(marker_name).exists() {
                root_candidate = Some(dir.to_path_buf());
                break 'outer;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    let resolved = root_candidate.ok_or_else(|| {
        CommandError::Internal(
            "Cannot locate workspace root: src-tauri/tauri.conf.json not found".into(),
        )
    })?;
    let canon = resolved.canonicalize().map_err(|e| {
        CommandError::Internal(format!("Failed to canonicalize workspace root: {}", e))
    })?;
    log::debug!(
        "[workspace_root] current_dir={:?} exe={:?} resolved={:?} canonicalized={:?}",
        raw,
        exe,
        resolved,
        canon
    );
    Ok(canon)
}

/// Key under which the user's trusted workspace roots are persisted, as a
/// JSON array of canonicalized absolute path strings.
///
/// Athena is a *multi-project* terminal launcher: every Space carries an
/// arbitrary working directory (`types::workspace::Space::dir`), and the whole
/// point is to run terminals and AI agents in user-chosen project folders.
/// The sandbox below therefore accepts any path descending from the app's own
/// project root *or* any trusted root added here. A root is added the moment
/// the user deliberately creates a Space for it — the authorization gesture.
const TRUSTED_ROOTS_KEY: &str = "workspace.trusted_roots";

/// Load the user's trusted workspace roots from the persistent store.
///
/// Each stored entry is re-canonicalized on load so that comparisons against a
/// canonicalized request path stay stable. Roots that no longer resolve (the
/// directory was moved/deleted/renamed) are silently skipped — they simply
/// can't authorize anything until re-added. A malformed or missing key yields
/// an empty list (first run, or a corrupt value); the store is never trusted
/// to hand back a canonicalized form.
fn load_trusted_roots(store: &athena_store::KeyValueStore) -> Vec<std::path::PathBuf> {
    let raw: Option<Vec<String>> = match store.get(TRUSTED_ROOTS_KEY) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[trusted_roots] failed to read key '{}': {}",
                TRUSTED_ROOTS_KEY,
                e
            );
            return Vec::new();
        }
    };
    match raw {
        Some(list) => list
            .into_iter()
            .filter_map(|p| {
                std::path::PathBuf::from(&p)
                    .canonicalize()
                    .map_err(|e| {
                        log::debug!(
                            "[trusted_roots] skipping '{}': canonicalize failed: {}",
                            p,
                            e
                        );
                        e
                    })
                    .ok()
            })
            .collect(),
        None => Vec::new(),
    }
}

/// The full set of roots a request path may descend from: the app's own
/// project root (always implicitly trusted) plus every user-added trusted
/// root. All entries are canonicalized.
fn effective_roots(store: &athena_store::KeyValueStore) -> Vec<std::path::PathBuf> {
    let mut roots = vec![get_workspace_root().unwrap_or_else(|_| std::path::PathBuf::from("/"))];
    roots.extend(load_trusted_roots(store));
    roots
}

/// True if `canonical` is equal to or a descendant of any root in `roots`.
///
/// `canonical` and every entry of `roots` must already be canonicalized
/// (symlinks resolved, no `..`) — which is exactly how the validators below
/// feed it. This preserves the existing traversal/symlink-escape guarantees;
/// we only widen the *set* of acceptable top-level directories, never the
/// canonicalization discipline. Exposed for unit testing.
fn is_within_any_root(canonical: &std::path::Path, roots: &[std::path::PathBuf]) -> bool {
    roots.iter().any(|r| canonical.starts_with(r))
}

/// Validate that a path exists, is inside the sandbox, and return its
/// canonical form.
///
/// The sandbox is the union of the app's project root and the user's trusted
/// workspace roots (see [`load_trusted_roots`]). The project root, every
/// trusted root, and the request path are all canonicalized before the
/// descendant check, so symlink escapes and `..` traversal are neutralized
/// exactly as before — only the set of permissible top-level directories
/// grows.
fn validate_path_exists(
    store: &athena_store::KeyValueStore,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let roots = effective_roots(store);
    // Relative paths resolve against the project root (first effective root).
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
            .join(path)
    };
    if !path.exists() {
        return Err(CommandError::NotFound("Path does not exist".to_string()));
    }
    let canonicalized = path
        .canonicalize()
        .map_err(|e| CommandError::Internal(format!("Failed to canonicalize path: {}", e)))?;
    if !is_within_any_root(&canonicalized, &roots) {
        // Do NOT echo the canonicalized workspace root or the requested path
        // back to the frontend — it confirms on-disk layout (user home
        // path, project location) to a probing renderer. Generic message only.
        return Err(CommandError::PermissionDenied(
            "Path is outside the workspace".to_string(),
        ));
    }
    Ok(canonicalized)
}

/// Validate a path for write operations (creates parent dirs if needed).
///
/// Tolerates a not-yet-existing leaf (the file we're about to write) by
/// canonicalizing its parent and re-joining the file name, then applies the
/// same multi-root descendant check as [`validate_path_exists`].
fn validate_path(
    store: &athena_store::KeyValueStore,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let roots = effective_roots(store);
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
            .join(path)
    };
    let canonical = if full_path.exists() {
        full_path
            .canonicalize()
            .map_err(|e| CommandError::Internal(format!("Failed to canonicalize path: {}", e)))?
    } else {
        let parent = full_path.parent().ok_or_else(|| {
            CommandError::InvalidInput(format!("path {:?} has no parent", full_path))
        })?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| CommandError::Internal(format!("Failed to canonicalize parent: {}", e)))?;
        match full_path.file_name() {
            Some(name) => canonical_parent.join(name),
            None => {
                return Err(CommandError::InvalidInput(format!(
                    "path {:?} has no file name",
                    full_path
                )))
            }
        }
    };
    if !is_within_any_root(&canonical, &roots) {
        return Err(CommandError::PermissionDenied(
            "Path is outside the workspace".to_string(),
        ));
    }
    if let Some(parent) = canonical.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CommandError::Internal(format!("Failed to create parent directories: {}", e))
        })?;
    }
    Ok(canonical)
}

// ── PTY spawn/write validation helpers ───────────────────────────────────────
//
// The IPC boundary is the entire frontend renderer. A compromised or XSSed
// renderer (note: CSP still permits `unsafe-eval`) could otherwise spawn an
// arbitrary binary rooted at `~/.ssh` or `/`, or paste unbounded data into a
// live PTY. These helpers gate the shell binary, the working directory, and
// payload sizes.

/// Maximum bytes accepted by `pty_write` / `pty_spawn_agent`'s `agent_cmd`.
/// Matches the cap used elsewhere for raw data payloads.
const MAX_PTY_DATA_BYTES: usize = 1024 * 1024; // 1 MB

/// Maximum length of a PTY session id. Generous; ids are caller-chosen.
const MAX_SESSION_ID_LEN: usize = 256;

/// Validate a shell binary path for PTY spawning.
///
/// Allowed if the canonicalized path lives under `/bin` or `/usr/bin`, or if it
/// matches the invoking user's `$SHELL` after canonicalization. This stops a
/// renderer from spawning `/usr/sbin/installer`, a homebrew binary, or an
/// arbitrary executable while still permitting standard shells (bash, zsh,
/// sh, fish when in /usr/bin).
fn validate_shell(shell: &str) -> Result<std::path::PathBuf, String> {
    if shell.is_empty() {
        return Err("shell path is empty".to_string());
    }
    let p = std::path::Path::new(shell);

    // Canonicalize the provided shell for comparison.
    let canon = p
        .canonicalize()
        .map_err(|e| format!("shell binary not accessible: {}", e))?;

    // Check $SHELL after canonicalization so an attacker can't set
    // SHELL=/tmp/evil and then invoke `pty_spawn` with that path.
    if let Ok(user_shell) = std::env::var("SHELL") {
        let user_canon = std::path::Path::new(&user_shell)
            .canonicalize()
            .map_err(|e| format!("$SHELL not accessible: {}", e))?;
        if canon == user_canon {
            return Ok(canon);
        }
    }

    // Canonicalize and require the binary to live under a system bin dir.
    // Using canonicalize (not lexical check) so symlinks are resolved: a
    // symlink in /bin pointing to /Users/x/evil is followed and then rejected
    // because the target isn't under /bin or /usr/bin.
    let allowed_ancestors = [
        std::path::Path::new("/bin"),
        std::path::Path::new("/usr/bin"),
    ];
    let ok = allowed_ancestors
        .iter()
        .any(|allowed| canon.starts_with(allowed));
    if !ok {
        return Err(format!(
            "shell binary '{}' is outside the allowed system directories (/bin, /usr/bin) and does not match $SHELL",
            shell
        ));
    }
    Ok(canon)
}

/// Validate a working directory for PTY spawning.
///
/// The directory must exist, be a directory, and be inside the sandbox (the
/// app project root ∪ trusted workspace roots). This stops a renderer from
/// spawning a shell rooted at `/`, `~/.ssh`, or an arbitrary path the user
/// never opted in to, while permitting every directory the user deliberately
/// turned into a Space.
fn validate_cwd(
    store: &athena_store::KeyValueStore,
    cwd: &str,
) -> Result<std::path::PathBuf, CommandError> {
    if cwd.is_empty() {
        return Err(CommandError::Internal("cwd is empty".to_string()));
    }
    let validated = validate_path_exists(store, std::path::Path::new(cwd))?;
    if !validated.is_dir() {
        return Err(CommandError::Internal(format!("cwd is not a directory")));
    }
    Ok(validated)
}

/// Validate a PTY session id: non-empty, bounded length, no control chars.
fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("session id is empty".to_string());
    }
    if id.len() > MAX_SESSION_ID_LEN {
        return Err(format!(
            "session id too long: {} > {}",
            id.len(),
            MAX_SESSION_ID_LEN
        ));
    }
    if id.chars().any(|c| c.is_control()) {
        return Err("session id contains control characters".to_string());
    }
    Ok(())
}

/// Validate a raw-data payload size for PTY writes.
fn validate_data_size(data: &[u8], label: &str) -> Result<(), String> {
    if data.len() > MAX_PTY_DATA_BYTES {
        return Err(format!(
            "{} too large: {} > {}",
            label,
            data.len(),
            MAX_PTY_DATA_BYTES
        ));
    }
    Ok(())
}

/// Infer the LLM provider from the configured base URL.
///
/// The Settings UI only collects a base URL + model (and an API key stored in
/// the keyring) — there is no explicit provider picker. Historically the
/// backend read a `llm.provider` key that nobody ever wrote, so every user was
/// silently routed through the OpenAI transport even when they meant to talk
/// to Anthropic. We now infer the provider from the host when the user hasn't
/// explicitly set `llm.provider`, and treat anything OpenAI-compatible
/// (Groq, OpenRouter, Together, local servers, …) as OpenAI.
fn infer_provider(base_url: &str, explicit: Option<&str>) -> athena_core::types::LLMProvider {
    use athena_core::types::LLMProvider;
    if let Some(p) = explicit {
        return match p.trim().to_ascii_lowercase().as_str() {
            "anthropic" => LLMProvider::Anthropic,
            "nvidia_nim" | "nvidia" | "nim" => LLMProvider::NvidiaNim,
            "lmstudio" | "lm_studio" | "lm-studio" => LLMProvider::Lmstudio,
            _ => LLMProvider::OpenAI,
        };
    }
    let host = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    if host.contains("anthropic.com") {
        LLMProvider::Anthropic
    } else if host == "localhost" && base_url.contains(":1234")
        || host.ends_with(".local")
        || (host == "localhost" && base_url.contains("/v1"))
    {
        // Heuristic: LM Studio's default port is 1234; Ollama lives on 11434.
        // We can't distinguish them perfectly, but LM Studio is the only
        // built-in provider with special behaviour (no vision), and it's the
        // one documented in the Settings placeholder.
        LLMProvider::Lmstudio
    } else if host.contains("integrate.api.nvidia.com") {
        LLMProvider::NvidiaNim
    } else {
        LLMProvider::OpenAI
    }
}

/// Distinct reasons we may fail to build a provider config. Splitting these
/// out lets the chat commands return a *specific* message ("set your API key")
/// rather than the orchestrator wandering into its `ANTHROPIC_API_KEY` env-var
/// fallback and failing with a confusing error far from the cause.
enum ProviderConfigError {
    /// No API key in the keyring and no legacy plaintext key to migrate.
    MissingApiKey,
}

/// Build provider config from the persistent store for LLM API calls.
///
/// Returns `Ok(config)` when everything needed is present, or
/// `Err(MissingApiKey)` when the user hasn't set a key. Other misconfiguration
/// (unknown provider string) logs a warning and falls back to a sensible
/// default instead of blocking all chat.
fn build_provider_config_from_store(
    state: &AppState,
) -> Result<athena_core::orchestrator::ProviderConfig, ProviderConfigError> {
    // An explicit provider key, if the user (or a future settings UI) set one,
    // overrides URL-based inference.
    let explicit_provider = state
        .store
        .get::<String>("llm.provider")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());

    let api_key = keyring::Entry::new("athena", "api_key")
        .ok()
        .and_then(|e| e.get_password().ok())
        .unwrap_or_default();

    if api_key.is_empty() {
        // Try migrating any legacy plaintext key that predates the keyring
        // integration before giving up.
        if let Ok(Some(value)) = state.store.get::<String>("llm.api_key") {
            if !value.is_empty() && value != "not_set" && value != "set" {
                if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
                    let _ = entry.set_password(&value);
                }
                let _ = state.store.delete_sync("llm.api_key");
                // Recurse once to pick up the freshly-migrated key without
                // duplicating the config-assembly logic below.
                return build_provider_config_from_store(state);
            }
        }
        log::warn!("No API key configured for LLM provider");
        return Err(ProviderConfigError::MissingApiKey);
    }

    let model = state
        .store
        .get::<String>("llm.model")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gpt-4o".to_string());
    let base_url = state
        .store
        .get::<String>("llm.base_url")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let provider = infer_provider(&base_url, explicit_provider.as_deref());

    Ok(athena_core::orchestrator::ProviderConfig::new(
        provider,
        api_key,
        model,
        String::new(),
        Some(base_url),
    ))
}

// ── Structured error type for Tauri commands ────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ── Window commands ──────────────────────────────────────────────────────────

/// Minimize the main application window.
#[tauri::command]
pub fn window_minimize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.minimize().map_err(|e| e.to_string())
}

/// Maximize or restore the main application window.
#[tauri::command]
pub fn window_maximize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.maximize().map_err(|e| e.to_string())
}

/// Close the main application window.
#[tauri::command]
pub fn window_close(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.close().map_err(|e| e.to_string())
}

/// Check whether the main window is currently maximized.
#[tauri::command]
pub fn window_is_maximized(app_handle: AppHandle) -> Result<bool, String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.is_maximized().map_err(|e| e.to_string())
}

/// Return the current platform identifier (e.g., `"macos"`, `"linux"`, `"windows"`).
#[tauri::command]
pub fn window_platform() -> String {
    std::env::consts::OS.to_string()
}

/// Return the default shell path for the current platform.
#[tauri::command]
pub fn pty_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "powershell.exe".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    })
}

// ── Trusted workspace roots ──────────────────────────────────────────────────
//
// The sandbox accepts paths descending from the app's project root or any
// trusted root the user opts into. A root is authorized here — the moment the
// user deliberately creates (or opens) a Space for a directory. The store
// keeps canonicalized paths; `load_trusted_roots` re-canonicalizes on read so
// a moved/renamed directory silently stops authorizing itself until re-added.

/// Add a directory to the set of trusted workspace roots.
///
/// This is the authorization gesture that lets a terminal or AI agent operate
/// in a directory outside the app's own project root. The directory must
/// exist and be a directory — a renderer cannot bless an arbitrary string;
/// it can only opt in to a real folder it could browse to anyway. Idempotent:
/// adding an already-trusted root is a no-op.
///
/// Canonicalizes before storing so later comparisons against a canonicalized
/// request path are exact, and so a symlinked path is stored as its target.
#[tauri::command]
pub async fn workspace_add_trusted_root(
    state: State<'_, AppState>,
    dir: String,
) -> Result<(), String> {
    // Resolve + stat on the blocking pool. Both `canonicalize` and `metadata`
    // touch the filesystem; doing them off the async runtime avoids stalling
    // the Tauri command executor on slow disks or network mounts.
    let dir_for_task = dir.clone();
    let (canonical, is_dir) = tokio::task::spawn_blocking(move || {
        let p = std::path::Path::new(&dir_for_task);
        let canonical = p.canonicalize();
        let is_dir = match &canonical {
            Ok(c) => std::fs::metadata(c).map(|m| m.is_dir()).unwrap_or(false),
            Err(_) => false,
        };
        (canonical, is_dir)
    })
    .await
    .map_err(|e| format!("path resolve task failed: {e}"))?;

    let canonical = canonical.map_err(|e| format!("'{}' is not accessible: {}", dir, e))?;
    if !is_dir {
        return Err(format!("'{}' is not a directory", dir));
    }
    let mut roots = load_trusted_roots(&state.store);
    if roots.iter().any(|r| r == &canonical) {
        return Ok(());
    }
    roots.push(canonical);
    let strs: Vec<String> = roots
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    state
        .store
        .set_sync(TRUSTED_ROOTS_KEY, &strs)
        .map_err(|e| format!("failed to persist trusted root: {}", e))?;
    Ok(())
}

/// Remove a directory from the set of trusted workspace roots.
///
/// Accepts either the stored canonical form or any path that canonicalizes to
/// it, so the frontend doesn't have to know the exact stored string.
/// Removing a root that isn't trusted is a no-op.
#[tauri::command]
pub async fn workspace_remove_trusted_root(
    state: State<'_, AppState>,
    dir: String,
) -> Result<(), String> {
    // Canonicalize on the blocking pool. If the directory no longer exists
    // (moved/deleted), canonicalize fails — but we still want to let the user
    // revoke trust, so fall back to the literal path string comparison against
    // whatever was stored.
    let dir_for_task = dir.clone();
    let canonical =
        tokio::task::spawn_blocking(move || std::path::Path::new(&dir_for_task).canonicalize())
            .await
            .map_err(|e| format!("canonicalize task failed: {e}"))?;
    let canonical = match canonical {
        Ok(c) => Some(c),
        Err(_) => None, // directory gone — best-effort literal remove below
    };

    let mut roots = load_trusted_roots(&state.store);
    let before = roots.len();
    if let Some(ref c) = canonical {
        roots.retain(|r| r != c);
    }
    // If canonical resolve failed, try a literal string match so a stale
    // entry for a deleted directory can still be cleared.
    if canonical.is_none() {
        let lit = std::path::PathBuf::from(&dir);
        roots.retain(|r| r != &lit);
    }
    if roots.len() == before {
        return Ok(()); // not present — no-op
    }
    let strs: Vec<String> = roots
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    state
        .store
        .set_sync(TRUSTED_ROOTS_KEY, &strs)
        .map_err(|e| format!("failed to persist trusted roots: {}", e))?;
    Ok(())
}

/// List the canonicalized trusted workspace roots.
#[tauri::command]
pub fn workspace_list_trusted_roots(state: State<'_, AppState>) -> Vec<String> {
    load_trusted_roots(&state.store)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

// ── File system commands ─────────────────────────────────────────────────────

/// Read the contents of a file as UTF-8 text.
#[tauri::command]
pub async fn fs_read_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, CommandError> {
    if !state.rate_limiter.check("fs_read_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    tokio::task::spawn_blocking(move || {
        // Check file size before reading to prevent memory exhaustion.
        let metadata = std::fs::metadata(&validated_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        if metadata.len() > caps::MAX_FS_READ_BYTES as u64 {
            return Err(CommandError::InvalidInput(format!(
                "file too large: {} bytes (max {})",
                metadata.len(),
                caps::MAX_FS_READ_BYTES
            )));
        }
        std::fs::read_to_string(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

#[derive(serde::Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// List the contents of a directory, sorted with directories first.
#[tauri::command]
pub async fn fs_list_dir(state: State<'_, AppState>, path: String) -> Result<String, CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    tokio::task::spawn_blocking(move || {
        let mut entries: Vec<DirEntry> = Vec::new();
        let read_dir =
            std::fs::read_dir(&validated).map_err(|e| CommandError::Internal(e.to_string()))?;
        for entry_result in read_dir {
            let entry = entry_result.map_err(|e| CommandError::Internal(e.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().to_string_lossy().to_string();
            let is_dir = file_type.is_dir();
            entries.push(DirEntry { name, path, is_dir });
        }
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        serde_json::to_string(&entries).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

/// Write content to a file, creating it if it doesn't exist.
#[tauri::command]
pub async fn fs_write_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> Result<(), CommandError> {
    if !state.rate_limiter.check("fs_write_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    if content.len() > caps::MAX_FS_WRITE_BYTES {
        return Err(CommandError::InvalidInput(format!(
            "content too large: {} > {}",
            content.len(),
            caps::MAX_FS_WRITE_BYTES
        )));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    let content_clone = content.clone();
    tokio::task::spawn_blocking(move || {
        // Atomic write: write to a temp file in the same directory, then rename.
        let temp_path = match validated_clone.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                parent.join(format!(".tmp-write-{}", uuid::Uuid::new_v4()))
            }
            _ => std::env::temp_dir().join(format!(".tmp-write-{}", uuid::Uuid::new_v4())),
        };
        std::fs::write(&temp_path, content_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        std::fs::rename(&temp_path, &validated_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Write task failed: {e}")))?
}

/// Check whether a path exists and is within the allowed directory.
///
/// Synchronous (not `async`) because Tauri forbids async commands that return
/// a bare non-`Result` type. Sync commands run on the runtime's blocking
/// thread, so the canonicalize inside `validate_path_exists` won't stall the
/// async executor.
#[tauri::command]
pub fn fs_exists(state: State<'_, AppState>, path: String) -> bool {
    let path_ref = std::path::Path::new(&path);
    validate_path_exists(&state.store, path_ref).is_ok()
}

/// Read a file and return its contents as a base64-encoded string.
#[tauri::command]
pub async fn fs_read_file_as_base64(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, CommandError> {
    if !state.rate_limiter.check("fs_read_file_as_base64") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    use base64::Engine;
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        // Check file size before reading to prevent memory exhaustion.
        let metadata = std::fs::metadata(&validated_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        if metadata.len() > caps::MAX_FS_READ_BYTES as u64 {
            return Err(CommandError::InvalidInput(format!(
                "file too large: {} bytes (max {})",
                metadata.len(),
                caps::MAX_FS_READ_BYTES
            )));
        }
        std::fs::read(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))??;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Show a native file/folder open dialog and return the selected path(s).
#[tauri::command]
pub async fn fs_show_open_dialog(
    app_handle: AppHandle,
    title: Option<String>,
    multiple: Option<bool>,
    directory: Option<bool>,
) -> Result<String, String> {
    let is_directory = directory.unwrap_or(false);
    let is_multiple = multiple.unwrap_or(false);

    let mut dialog = app_handle.dialog().file();
    if let Some(t) = &title {
        dialog = dialog.set_title(t);
    }

    let result = tokio::task::spawn_blocking(move || match (is_directory, is_multiple) {
        (true, false) => dialog
            .blocking_pick_folder()
            .map(|fp| fp.to_string())
            .unwrap_or_default(),
        (true, true) => dialog
            .blocking_pick_folders()
            .map(|list| {
                list.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        (false, false) => dialog
            .blocking_pick_file()
            .map(|fp| fp.to_string())
            .unwrap_or_default(),
        (false, true) => dialog
            .blocking_pick_files()
            .map(|list| {
                list.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
    })
    .await
    .map_err(|e| format!("Dialog task failed: {e}"))?;

    Ok(result)
}

/// Show a native file dialog filtered to image types (png, jpg, jpeg, gif, svg, webp).
#[tauri::command]
pub async fn fs_show_image_dialog(app_handle: AppHandle) -> Result<String, String> {
    let dialog = app_handle
        .dialog()
        .file()
        .set_title("Select Image")
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "svg", "webp"]);

    let result = tokio::task::spawn_blocking(move || {
        dialog
            .blocking_pick_file()
            .map(|fp| fp.to_string())
            .unwrap_or_default()
    })
    .await
    .map_err(|e| format!("Dialog task failed: {e}"))?;

    Ok(result)
}

/// Search files in a directory using ripgrep with the given pattern.
#[tauri::command]
pub async fn fs_search_files(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref).map_err(|e| e.to_string())?;
    let options = athena_core::SearchOptions {
        pattern,
        path: validated.to_string_lossy().to_string(),
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Store commands ───────────────────────────────────────────────────────────

/// Get a value from the persistent key-value store.
#[tauri::command]
pub fn store_get(state: State<'_, AppState>, key: String) -> Result<String, CommandError> {
    caps::validate_key(&key).map_err(CommandError::InvalidInput)?;
    if key == "llm.api_key" {
        // Fast path: check the confirmation flag written by store_set.
        // This avoids an expensive (and permission-dependent) keyring access
        // on every panel mount.
        if let Ok(Some(ref s)) = state.store.get::<String>("llm.api_key_status") {
            if s == "set" {
                log::debug!("[store_get] llm.api_key => confirmed by flag");
                return Ok("set".to_string());
            }
        }
        // Check keyring second
        if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
            match entry.get_password() {
                Ok(_) => {
                    log::debug!("[store_get] llm.api_key => found in OS keyring");
                    // Repair the confirmation flag in case it was lost
                    let _ = state.store.set_sync("llm.api_key_status", &"set");
                    return Ok("set".to_string());
                }
                Err(keyring::Error::NoEntry) => {
                    // Key genuinely doesn't exist; nothing to do
                }
                Err(e) => {
                    log::warn!("[store_get] llm.api_key => keyring read failed: {}", e);
                }
            }
        } else {
            log::warn!("[store_get] llm.api_key => failed to open keyring entry");
        }
        // Fallback: check store for a legacy plaintext key and migrate it
        if let Ok(Some(value)) = state.store.get::<String>(&key) {
            if !value.is_empty() && value != "not_set" && value != "set" {
                // Found a raw key in the store — migrate to keyring
                if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
                    let _ = entry.set_password(&value);
                }
                // Delete the plaintext key from the store so it never leaks again
                let _ = state.store.delete_sync(&key);
                // Write the confirmation flag too
                let _ = state.store.set_sync("llm.api_key_status", &"set");
                return Ok("set".to_string());
            }
        }
        return Ok("not_set".to_string());
    }

    state
        .store
        .get::<String>(&key)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Key '{}' not found", key)))
}

/// Set a value in the persistent key-value store.
#[tauri::command]
pub fn store_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    caps::validate_key(&key)?;
    // Block writes to sensitive key namespaces from the frontend to prevent
    // key tampering and unauthorized secrets storage.
    const FORBIDDEN_PREFIXES: &[&str] = &["secret.", "auth.", "password.", "credential."];
    for prefix in FORBIDDEN_PREFIXES {
        if key.starts_with(prefix) {
            return Err(format!(
                "Writing to key namespace '{}' is forbidden",
                prefix
            ));
        }
    }
    if key == "llm.api_key" {
        if !value.is_empty() && value != "set" && value != "not_set" {
            // Store the API key securely in the OS keyring, never in plaintext
            let entry = keyring::Entry::new("athena", "api_key")
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            entry
                .set_password(&value)
                .map_err(|e| format!("Failed to store API key in keyring: {}", e))?;
            log::info!(
                "[store_set] API key saved to OS keyring (service='athena', account='api_key')"
            );
            // Remove any legacy plaintext key from the store
            let _ = state.store.delete_sync(&key);
            // Write a lightweight confirmation flag so the frontend can
            // check key status without hitting the keyring (avoids keychain
            // lockout / permission-denied races on mount).
            let _ = state.store.set_sync("llm.api_key_status", &"set");
        } else if value.is_empty() || value == "not_set" {
            // Clear the API key from the keyring
            let entry = keyring::Entry::new("athena", "api_key")
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            let _ = entry.delete_credential();
            log::info!(
                "[store_set] API key removed from OS keyring (service='athena', account='api_key')"
            );
            let _ = state.store.delete_sync(&key);
            let _ = state.store.delete_sync("llm.api_key_status");
        }
        return Ok(());
    }

    state
        .store
        .set_sync(&key, &value)
        .map_err(|e| e.to_string())
}

/// Check whether a key exists in the persistent key-value store.
#[tauri::command]
pub fn store_has(state: State<'_, AppState>, key: String) -> bool {
    if key == "llm.api_key" {
        if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
            if entry.get_password().is_ok() {
                return true;
            }
        }
    }
    state.store.has(&key)
}

/// Delete a key from the persistent key-value store.
#[tauri::command]
pub fn store_delete(state: State<'_, AppState>, key: String) -> Result<(), String> {
    caps::validate_key(&key)?;
    if key == "llm.api_key" {
        let entry = keyring::Entry::new("athena", "api_key")
            .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
        let _ = entry.delete_credential();
    }
    state.store.delete_sync(&key).map_err(|e| e.to_string())
}

/// Test whether the LLM API key can be read from the keyring and return a
/// structured result for the Settings UI to display.
#[tauri::command]
pub fn test_llm_api_key(state: State<'_, AppState>) -> Result<String, String> {
    // Returns a serialized JSON string ({ ok, message }) — the frontend bridge
    // casts the IPC result to a String and parses it with serde_json::from_str,
    // so this command must emit a JSON *string*, not a bare JSON object.
    let result = test_llm_api_key_value(state);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Inner implementation returning the structured JSON value. Kept separate so the
/// logic is unit-testable without going through string serialization.
fn test_llm_api_key_value(state: State<'_, AppState>) -> serde_json::Value {
    // 1. Fast path: if the confirmation flag is missing, the key was never saved
    match state.store.get::<String>("llm.api_key_status") {
        Ok(Some(ref s)) if s == "set" => { /* fall through to keyring test */ }
        _ => {
            return serde_json::json!({
                "ok": false,
                "message": "No API key configured. Save one in Settings first."
            });
        }
    }

    // 2. Try to read the keyring
    match keyring::Entry::new("athena", "api_key") {
        Ok(entry) => match entry.get_password() {
            Ok(key) => {
                if key.is_empty() {
                    serde_json::json!({
                        "ok": false,
                        "message": "API key is empty in keyring. Re-save it in Settings."
                    })
                } else {
                    serde_json::json!({
                        "ok": true,
                        "message": "API key read successfully from keyring."
                    })
                }
            }
            Err(keyring::Error::NoEntry) => serde_json::json!({
                "ok": false,
                "message": "API key not found in keyring. Save it again in Settings."
            }),
            Err(e) => {
                log::warn!("[test_llm_api_key] keyring read failed: {}", e);
                serde_json::json!({
                    "ok": false,
                    "message": format!("Keychain access failed: {}. Unlock your keychain and try again.", e)
                })
            }
        },
        Err(e) => serde_json::json!({
            "ok": false,
            "message": format!("Failed to open keyring entry: {}", e)
        }),
    }
}

// ── Session commands ─────────────────────────────────────────────────────────

/// Create a new chat session and return its JSON representation.
#[tauri::command]
pub async fn session_create(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    if let Some(ref t) = title {
        caps::validate_title(t)?;
    }
    let session = state
        .session_store
        .create_session(title.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&session).map_err(|e| e.to_string())
}

/// Get a chat session by its ID.
#[tauri::command]
pub async fn session_get(state: State<'_, AppState>, id: String) -> Result<String, CommandError> {
    let session = state
        .session_store
        .get_session(&id)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// List all chat sessions with summary information (id, title, message count, etc.).
#[tauri::command]
pub async fn session_list(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state
        .session_store
        .list_sessions()
        .await
        .map_err(|e| e.to_string())?;
    let mut json = Vec::new();
    for item in &sessions {
        json.push(serde_json::json!({
            "id": item.id,
            "title": item.title,
            "createdAt": item.created_at,
            "updatedAt": item.updated_at,
            "messageCount": item.message_count,
            "lastMessagePreview": item.last_message_preview
        }));
    }
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

/// Delete a chat session by its ID.
#[tauri::command]
pub async fn session_delete(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .session_store
        .delete_session(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("deleted".to_string())
}

/// Update a chat session's title and/or messages.
#[tauri::command]
pub async fn session_update(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    messages: Option<String>,
) -> Result<String, CommandError> {
    if let Some(ref t) = title {
        caps::validate_title(t).map_err(CommandError::InvalidInput)?;
    }
    let parsed_messages: Option<Vec<athena_store::SessionMessage>> = match messages {
        Some(json) => Some(
            serde_json::from_str(&json)
                .map_err(|e| CommandError::InvalidInput(format!("Invalid messages JSON: {}", e)))?,
        ),
        None => None,
    };
    let session = state
        .session_store
        .update_session(&id, title.as_deref(), parsed_messages)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// Add a message to an existing chat session.
#[tauri::command]
pub async fn session_add_message(
    state: State<'_, AppState>,
    session_id: String,
    role: String,
    content: String,
    is_error: Option<bool>,
    image_refs: Option<String>,
) -> Result<String, String> {
    let mut session = state
        .session_store
        .get_session(&session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Session not found".to_string())?;

    let parsed_refs: Option<Vec<athena_store::ImageRef>> = match image_refs {
        Some(json) => Some(serde_json::from_str(&json).map_err(|e| e.to_string())?),
        None => None,
    };

    let message_role = match role.as_str() {
        "user" => athena_store::MessageRole::User,
        _ => athena_store::MessageRole::Athena,
    };

    let msg = athena_store::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: message_role,
        content,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        is_error,
        image_refs: parsed_refs,
    };

    session.messages.push(msg);
    let updated = state
        .session_store
        .update_session(&session_id, None, Some(session.messages))
        .await
        .map_err(|e| e.to_string())?;
    match updated {
        Some(s) => serde_json::to_string(&s).map_err(|e| e.to_string()),
        None => Err("Failed to update session".to_string()),
    }
}

// ── PTY commands ─────────────────────────────────────────────────────────────

/// Spawn a new PTY session with the given ID, working directory, and shell.
/// After spawning, starts a background tokio task that reads PTY output
/// and emits `terminal:data` events to the frontend.
#[tauri::command]
pub async fn pty_spawn(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    log::info!(
        "pty_spawn requested: id={} cwd={} shell={} cols={} rows={}",
        id,
        cwd,
        shell,
        cols,
        rows
    );
    // Validate caller-supplied values before touching the session manager.
    validate_session_id(&id).map_err(|e| {
        log::warn!("pty_spawn rejected (bad id): {}", e);
        e
    })?;
    let validated_shell = validate_shell(&shell).map_err(|e| {
        log::warn!("pty_spawn rejected (bad shell '{}'): {}", shell, e);
        e
    })?;
    let validated_cwd = validate_cwd(&state.store, &cwd).map_err(|e| {
        log::warn!("pty_spawn rejected (bad cwd '{}'): {}", cwd, e);
        e.to_string()
    })?;
    let shell_str = validated_shell.to_string_lossy().to_string();
    let cwd_str = validated_cwd.to_string_lossy().to_string();

    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn(id.clone(), &shell_str, &cwd_str, cols, rows)
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = state.app_handle.lock().clone();

            if let Some(handle) = app_handle {
                let session_id_for_loop = id.clone();
                let output_buffer = std::sync::Arc::clone(&state.output_buffer);
                tokio::spawn(async move {
                    pty_read_loop(handle, session_id_for_loop, session, output_buffer).await;
                });
            }

            log::info!(
                "PTY session spawned: id={} cwd={} shell={} cols={} rows={}",
                id,
                cwd,
                shell,
                cols,
                rows
            );
            Ok(())
        }
        Err(e) => {
            log::error!(
                "Failed to spawn PTY session: id={} cwd={} shell={} cols={} rows={} error={}",
                id,
                cwd,
                shell,
                cols,
                rows,
                e
            );
            Err(e.to_string())
        }
    }
}

/// Background task that reads PTY output and emits Tauri events.
///
/// Fans out to two parallel event streams:
/// - `pty:raw` — base64-encoded raw PTY bytes, consumed by the xterm.js
///   frontend (which has its own ANSI parser). Emitted in coalesced
///   batches (one event per flush) to reduce per-event overhead and
///   give the frontend larger, more stable chunks to render.
/// - `terminal:data` — parsed cell deltas, consumed by the legacy
///   cell-grid frontend. Emitted only when the grid actually changed.
pub(crate) async fn pty_read_loop(
    app_handle: tauri::AppHandle,
    session_id: String,
    session: std::sync::Arc<athena_terminal::session::TerminalSession>,
    output_buffer: std::sync::Arc<athena_core::output_buffer::OutputBuffer>,
) {
    log::info!("pty_read_loop[{}]: starting", session_id);
    let mut did_emit_ready = false;

    // 16 KB read buffer — reduces per-read syscall overhead while staying
    // below typical kernel pagecache sizes for good latency.
    let mut read_buf = vec![0u8; 16 * 1024];

    // Coalescing buffer for `pty:raw` PTY output. Pre-allocate to 32 KB
    // to avoid reallocation churn during active output.
    let mut coalesce_buf: Vec<u8> = Vec::with_capacity(32 * 1024);
    // Hard cap on coalescing buffer (1 MB).  If the buffer ever exceeds
    // this size we emit immediately to avoid unbounded memory growth.
    const MAX_COALESCE_SIZE: usize = 1024 * 1024; // 1 MB

    // 8 ms flush interval — balances latency with batching efficiency.
    let mut flush_interval = tokio::time::interval(tokio::time::Duration::from_millis(8));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    /// Flush accumulated raw PTY bytes as a single `pty:raw` event.
    fn flush_pty_raw(coalesce_buf: &mut Vec<u8>, app_handle: &tauri::AppHandle, session_id: &str) {
        if coalesce_buf.is_empty() {
            return;
        }
        // NOTE: Tauri's `emit` serializes payloads to JSON before crossing
        // the IPC boundary. JSON cannot natively carry raw byte arrays,
        // so we must base64-encode. Passing `Vec<u8>` directly would only
        // result in an array-of-numbers JSON payload (more expensive to
        // parse and emit than a compact base64 string). True ArrayBuffer
        // transfer (ZeroCopy) over Tauri's `postMessage` IPC is not
        // supported by `tauri::Emitter::emit`, so base64 is the optimal
        // serialization for this event type.
        let encoded = base64::engine::general_purpose::STANDARD.encode(coalesce_buf.as_slice());
        let raw_event = serde_json::json!({
            "sessionId": session_id,
            "data": encoded,
        });
        // Serialize to a fully-owned String before calling emit. Passing
        // `&raw_event` (a `&serde_json::Value`) to emit captured a borrow
        // that, across concurrent tokio tasks, was observed to be read
        // after a later task had overwritten the underlying buffer — all
        // listeners then received payloads whose `sessionId` field matched
        // whichever task had last serialized. Owning the String forces
        // serialization to happen on this task, eliminating the race.
        let raw_event_str = match serde_json::to_string(&raw_event) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "pty_read_loop[{}]: failed to serialize raw_event: {}",
                    session_id,
                    e
                );
                coalesce_buf.clear();
                return;
            }
        };
        if let Err(e) = app_handle.emit("pty:raw", raw_event_str) {
            log::warn!("Failed to emit pty:raw event: {}", e);
        }
        coalesce_buf.clear();
    }

    loop {
        let n: usize = tokio::select! {
            // `biased` ensures the read branch is preferred when data is
            // available, so we drain the PTY eagerly without dropping
            // completed reads because of an interval tick.
            biased;

            result = session.read_bytes(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        // `Ok(0)` on a non-blocking fd means no data is
                        // available (EAGAIN) — a lull in output. Flush any
                        // pending coalesced data so the frontend gets prompt
                        // feedback after a burst of output.
                        if !coalesce_buf.is_empty() {
                            flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                        continue;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        // Flush any pending data before handling the error
                        // so the frontend doesn't lose the tail of output.
                        if !coalesce_buf.is_empty() {
                            flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
                        }
                        log::warn!("PTY read error for {}: {}", session_id, e);
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.kind() == std::io::ErrorKind::InvalidData
                        {
                            break;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        continue;
                    }
                }
            }

            _ = flush_interval.tick() => {
                // Timer-based flush: guarantees the frontend receives data
                // at least every 8 ms, even during a slow trickle where
                // `read_bytes` never returns `Ok(0)`.
                if !coalesce_buf.is_empty() {
                    flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
                }
                continue;
            }
        };

        log::trace!("pty_read_loop[{}]: read {} bytes", session_id, n);

        // Convert raw PTY bytes to text and append to output buffer
        let text = String::from_utf8_lossy(&read_buf[..n]);
        output_buffer.append_output(&session_id, &text, None);

        // Step 1: parse the same bytes for the legacy cell-grid frontend.
        // `parse_bytes` returns `None` when no cells changed, in which
        // case we skip the structured event entirely.
        // For xterm.js sessions, we still parse (to keep VTE state fresh)
        // but skip emitting `terminal:data` — xterm.js parses raw ANSI itself.
        match session.parse_bytes(&read_buf[..n]).await {
            Ok(Some(update)) => {
                if !did_emit_ready {
                    did_emit_ready = true;
                    session.mark_ready().await;
                    // Clone to an owned String to avoid the same borrow-sharing
                    // race that motivated the pty:raw String-serialize fix.
                    if let Err(e) = app_handle.emit("terminal:ready", session_id.clone()) {
                        log::warn!("Failed to emit terminal:ready event: {}", e);
                    }
                }

                // Skip cell-delta emission for xterm sessions — they have their
                // own ANSI parser and do not consume `terminal:data` events.
                if session.is_xterm.load(std::sync::atomic::Ordering::Relaxed) {
                    // still need to emit `terminal:ready` above, but skip data
                } else {
                    let event_data = serde_json::json!({
                        "sessionId": session_id,
                        "deltas": update.deltas,
                        "cursorRow": update.cursor_row,
                        "cursorCol": update.cursor_col,
                        "rows": update.rows,
                        "cols": update.cols,
                        "cursorVisible": update.cursor_visible,
                    });
                    let event_data_str = match serde_json::to_string(&event_data) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!(
                                "pty_read_loop[{}]: failed to serialize event_data: {}",
                                session_id,
                                e
                            );
                            continue;
                        }
                    };
                    if let Err(e) = app_handle.emit("terminal:data", event_data_str) {
                        log::warn!("Failed to emit terminal:data event: {}", e);
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!("PTY parse error for {}: {}", session_id, e);
            }
        }

        // Step 2: accumulate raw bytes into the coalescing buffer.
        coalesce_buf.extend_from_slice(&read_buf[..n]);

        // Step 3: size-threshold flush — prevents unbounded growth when
        // commands like `yes` produce continuous output.
        // Emergency flush at the hard 1 MB cap, normal flush at 32 KB.
        if coalesce_buf.len() >= MAX_COALESCE_SIZE {
            flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
        } else if coalesce_buf.len() >= 32 * 1024 {
            flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
        }

        // Rate limit: yield after each successful read to prevent CPU spin
        // when commands like `yes` produce infinite output.
        tokio::task::yield_now().await;
    }

    log::info!("PTY read loop exited for session: {}", session_id);

    // Flush any remaining coalesced data before signaling exit so the
    // frontend doesn't miss the tail of the session's output.
    if !coalesce_buf.is_empty() {
        flush_pty_raw(&mut coalesce_buf, &app_handle, &session_id);
    }

    if let Err(e) = app_handle.emit("terminal:exit", session_id) {
        log::warn!("Failed to emit terminal:exit event: {}", e);
    }
}

/// Write data to a PTY session's stdin.
#[tauri::command]
pub async fn pty_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    validate_session_id(&id)?;
    let data_len = data.len();
    validate_data_size(data.as_bytes(), "pty_write data")?;
    let session_manager = state.session_manager.lock().await;
    let _len = session_manager
        .write(&id, data.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    drop(session_manager);
    Ok(())
}

/// Kill a PTY session by its ID.
#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.kill(&id).await;
    drop(session_manager);
    result.map_err(|e| e.to_string())
}

/// Resize a PTY session's terminal dimensions.
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    log::info!(
        "pty_resize requested: id={} cols={} rows={}",
        id,
        cols,
        rows
    );
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.resize(&id, cols, rows).await;
    drop(session_manager);
    result.map_err(|e| e.to_string())
}

/// Get the accumulated output history for a PTY session.
/// Returns the current grid state as a JSON array of rows with cell characters.
#[tauri::command]
pub async fn pty_get_history(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        let grid = s.grid.lock().await;
        let mut rows_json = Vec::new();
        for row in &grid.rows {
            let chars: Vec<String> = row.iter().map(|c| c.c.to_string()).collect();
            rows_json.push(serde_json::json!({ "cells": chars }));
        }
        return serde_json::to_string(&serde_json::json!({
            "rows": rows_json,
            "cursor_row": grid.cursor.row,
            "cursor_col": grid.cursor.col,
        }))
        .map_err(|e| e.to_string());
    }
    Ok("null".to_string())
}

/// Check whether a PTY session with the given ID exists.
#[tauri::command]
pub async fn pty_has_session(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.has_session(&id).await;
    drop(session_manager);
    Ok(result)
}

/// Check whether a PTY session's shell prompt is visible (ready).
/// Returns true only when the session status is Ready (shell has started).
#[tauri::command]
pub async fn pty_is_ready(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = match session_manager.get_session(&id).await {
        Some(session) => {
            let status = session.status.lock().await;
            *status == athena_terminal::session::PtyStatus::Ready
        }
        None => false,
    };
    drop(session_manager);
    Ok(result)
}

/// Get the working directory of a PTY session, if known.
#[tauri::command]
pub async fn pty_get_cwd(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        Ok(Some(s.cwd.clone()))
    } else {
        Ok(None)
    }
}

/// Structured info about a PTY session's current foreground process.
#[derive(Debug, Clone, serde::Serialize)]
struct AgentInfo {
    foreground_process: String,
    task_title: Option<String>,
    /// Session ID from the agent's history file, used to avoid
    /// re-summarizing the same session on every poll.
    session_id: Option<String>,
    /// Unix timestamp (ms) of the last prompt for the session.
    timestamp: Option<u64>,
    /// Raw prompt text (available for LLM summarization). Only set when
    /// the feature is enabled so the frontend can call the summarizer.
    raw_prompt: Option<String>,
}

/// Metadata scraped from the last entry in Claude's history file.
#[derive(Debug, Clone)]
struct ClaudeHistoryEntry {
    /// The raw prompt text the user typed.
    display: String,
    /// The session UUID this prompt belongs to.
    session_id: String,
    /// Unix timestamp (ms) when the prompt was sent.
    timestamp: u64,
}

/// Scrape the last session entry from Claude's history file.
/// Reads `~/.claude/history.jsonl` and returns the `display`, `sessionId`
/// and `timestamp` fields of the last line.
fn scrape_claude_task() -> Option<ClaudeHistoryEntry> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".claude/history.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    let last_line = content.lines().last()?;
    let json: serde_json::Value = serde_json::from_str(last_line).ok()?;
    let display = json.get("display")?.as_str()?.trim().to_string();
    let session_id = json.get("sessionId")?.as_str()?.to_string();
    let timestamp = json.get("timestamp")?.as_u64()?;
    Some(ClaudeHistoryEntry {
        display,
        session_id,
        timestamp,
    })
}

/// Scrape the latest thread name from Codex's session index.
/// Reads `~/.codex/session_index.jsonl` and returns the `thread_name` of the last line.
fn scrape_codex_task() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".codex/session_index.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    let last_line = content.lines().last()?;
    let json: serde_json::Value = serde_json::from_str(last_line).ok()?;
    json.get("thread_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
}

/// Get the active foreground process and, if it's a known agent, try to
/// extract its current task title from the agent's own state files.
#[tauri::command]
pub async fn pty_agent_info(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    let Some(s) = session else {
        let info = AgentInfo {
            foreground_process: "shell".to_string(),
            task_title: None,
            session_id: None,
            timestamp: None,
            raw_prompt: None,
        };
        return serde_json::to_string(&info).map_err(|e| e.to_string());
    };

    // Use tcgetpgrp to get the ACTUAL foreground process group of the
    // controlling terminal, not the shell's stored pgid. When the user runs
    // `claude` interactively, zsh/bash job control puts it into a new
    // process group. tcgetpgrp(master_fd) returns that foreground group.
    let mut pgid = s.pgid.as_raw();
    let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
    if master_fd >= 0 {
        let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
        if fg_pgid > 0 {
            pgid = fg_pgid;
        }
    }
    if pgid <= 0 {
        let info = AgentInfo {
            foreground_process: "shell".to_string(),
            task_title: None,
            session_id: None,
            timestamp: None,
            raw_prompt: None,
        };
        return serde_json::to_string(&info).map_err(|e| e.to_string());
    }

    // Get the full command line for each process in the PTY's process group.
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ps")
            .args(&["-o", "command=", "-g", &pgid.to_string()])
            .output()
    })
    .await
    .map_err(|e| e.to_string())?;

    let process = match output {
        Ok(out) if out.status.success() => {
            let shell_names: std::collections::HashSet<&str> =
                ["sh", "bash", "zsh", "fish", "csh", "tcsh"]
                    .iter()
                    .cloned()
                    .collect();

            let lines: Vec<&str> = std::str::from_utf8(&out.stdout)
                .unwrap_or("")
                .lines()
                .collect();

            let mut detected = "shell".to_string();
            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // First whitespace-separated word is the binary path/name
                let first_word = trimmed.split_whitespace().next().unwrap_or("");
                let comm_name = std::path::Path::new(first_word)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(first_word);

                // Skip shell processes
                if shell_names.contains(comm_name) {
                    continue;
                }

                // Detect node-based tools (claude, codex, opencode, etc.)
                let lower = trimmed.to_lowercase();
                detected = if comm_name == "node" || comm_name.ends_with("node") {
                    if lower.contains("claude") {
                        "claude".to_string()
                    } else if lower.contains("codex") {
                        "codex".to_string()
                    } else if lower.contains("opencode") {
                        "opencode".to_string()
                    } else {
                        "node".to_string()
                    }
                } else {
                    comm_name.to_string()
                };
                break;
            }
            detected
        }
        _ => "shell".to_string(),
    };

    // Try to extract the task title and session metadata from the agent's
    // own state files.
    let (task_title, session_id, timestamp, raw_prompt) = match process.as_str() {
        "claude" => match scrape_claude_task() {
            Some(entry) => {
                let raw = entry.display.clone();
                (
                    Some(entry.display),
                    Some(entry.session_id),
                    Some(entry.timestamp),
                    Some(raw),
                )
            }
            None => (None, None, None, None),
        },
        "codex" => (scrape_codex_task(), None, None, None),
        _ => (None, None, None, None),
    };

    let info = AgentInfo {
        foreground_process: process,
        task_title,
        session_id,
        timestamp,
        raw_prompt,
    };
    serde_json::to_string(&info).map_err(|e| e.to_string())
}

/// Get the name of the active foreground process under a PTY session.
/// Uses `lsof` to find which command currently has the PTY's tty open.
/// Returns `None` if the session doesn't exist or the foreground cannot be determined.
#[tauri::command]
pub async fn pty_foreground_process(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        // Use tcgetpgrp to get the ACTUAL foreground process group of the
        // controlling terminal, not the shell's stored pgid.
        let mut pgid = s.pgid.as_raw();
        let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
        if master_fd >= 0 {
            let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
            if fg_pgid > 0 {
                pgid = fg_pgid;
            }
        }
        if pgid <= 0 {
            return Ok("shell".to_string());
        }

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("ps")
                .args(&["-o", "command=", "-g", &pgid.to_string()])
                .output()
        })
        .await
        .map_err(|e| e.to_string())?;

        match output {
            Ok(out) if out.status.success() => Ok(classify_foreground_ps(
                std::str::from_utf8(&out.stdout).unwrap_or(""),
            )),
            _ => Ok("shell".to_string()),
        }
    } else {
        Ok("shell".to_string())
    }
}

/// Agent CLI labels that [`classify_foreground_ps`] can report. A session whose
/// foreground process classifies to one of these is treated as an agent pane
/// during app-exit resume capture.
const AGENT_FG_NAMES: &[&str] = &["claude", "codex", "opencode", "gemini"];

/// Classify the foreground command(s) reported by `ps -o command= -g <pgid>`
/// into an agent/shell label. Returns `"shell"` when only a shell (or nothing
/// recognizable) is running. Pure over the `ps` stdout so it is unit-testable.
fn classify_foreground_ps(stdout: &str) -> String {
    let shell_names: std::collections::HashSet<&str> = ["sh", "bash", "zsh", "fish", "csh", "tcsh"]
        .into_iter()
        .collect();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let first_word = trimmed.split_whitespace().next().unwrap_or("");
        let comm_name = std::path::Path::new(first_word)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(first_word);
        if shell_names.contains(comm_name) {
            continue;
        }
        let lower = trimmed.to_lowercase();
        return if comm_name == "node" || comm_name.ends_with("node") {
            if lower.contains("claude") {
                "claude".to_string()
            } else if lower.contains("codex") {
                "codex".to_string()
            } else if lower.contains("opencode") {
                "opencode".to_string()
            } else {
                "node".to_string()
            }
        } else {
            comm_name.to_string()
        };
    }
    "shell".to_string()
}

/// Best-effort classification of a single live session's controlling-terminal
/// foreground process (covers `claude` typed inside a plain shell pane).
/// Returns a label like `"claude"`/`"shell"`. Used by the app-exit capture to
/// decide which panes are agents worth nudging with `/exit`.
async fn session_foreground_label(
    session: &Arc<athena_terminal::session::TerminalSession>,
) -> String {
    let mut pgid = session.pgid.as_raw();
    let master_fd = session.master_fd.load(std::sync::atomic::Ordering::Acquire);
    if master_fd >= 0 {
        let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
        if fg_pgid > 0 {
            pgid = fg_pgid;
        }
    }
    if pgid <= 0 {
        return "shell".to_string();
    }
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ps")
            .args(["-o", "command=", "-g", &pgid.to_string()])
            .output()
    })
    .await;
    match output {
        Ok(Ok(out)) if out.status.success() => {
            classify_foreground_ps(std::str::from_utf8(&out.stdout).unwrap_or(""))
        }
        _ => "shell".to_string(),
    }
}

/// Spawn a new PTY session with the agent command to execute after startup.
/// The `agent_cmd` is executed in the shell after the PTY is set up.
#[tauri::command]
pub async fn pty_spawn_agent(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    agent_cmd: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    // Validate caller-supplied values (same gates as pty_spawn) plus bound the
    // agent command payload before it is written to the PTY.
    validate_session_id(&id).map_err(|e| {
        log::warn!("pty_spawn_agent rejected (bad id): {}", e);
        e
    })?;
    let validated_shell = validate_shell(&shell).map_err(|e| {
        log::warn!("pty_spawn_agent rejected (bad shell '{}'): {}", shell, e);
        e
    })?;
    let validated_cwd = validate_cwd(&state.store, &cwd).map_err(|e| {
        log::warn!("pty_spawn_agent rejected (bad cwd '{}'): {}", cwd, e);
        e.to_string()
    })?;
    validate_data_size(agent_cmd.as_bytes(), "agent_cmd")?;
    let shell_str = validated_shell.to_string_lossy().to_string();
    let cwd_str = validated_cwd.to_string_lossy().to_string();

    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn(id.clone(), &shell_str, &cwd_str, cols, rows)
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = state.app_handle.lock().clone();

            // Write the agent command to the PTY
            if let Err(e) = session.write(agent_cmd.as_bytes()).await {
                log::error!("Failed to write agent command to PTY: {}", e);
                return Err(e.to_string());
            }

            if let Some(handle) = app_handle {
                let session_id_for_loop = id.clone();
                let output_buffer = std::sync::Arc::clone(&state.output_buffer);
                tokio::spawn(async move {
                    pty_read_loop(handle, session_id_for_loop, session, output_buffer).await;
                });
            }

            log::info!("PTY agent session spawned: id={}", id);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to spawn PTY agent session: {}", e);
            Err(e.to_string())
        }
    }
}

/// Mark a PTY session as being rendered by xterm.js.
///
/// When a session is xterm-backed, the backend skips emitting the
/// `terminal:data` cell-delta events because xterm.js parses raw ANSI
/// bytes itself.  This eliminates wasted VTE work, JSON serialization,
/// and IPC for those sessions.
#[tauri::command]
pub async fn pty_set_xterm(
    state: State<'_, AppState>,
    id: String,
    is_xterm: bool,
) -> Result<(), String> {
    let sm = state.session_manager.lock().await;
    if let Some(session) = sm.get_session(&id).await {
        session
            .is_xterm
            .store(is_xterm, std::sync::atomic::Ordering::Relaxed);
        log::debug!("pty_set_xterm: {} -> {}", id, is_xterm);
        Ok(())
    } else {
        Err(format!("Session {} not found", id))
    }
}

// ── Athena / Orchestrator commands ───────────────────────────────────────────

/// Send a text message to the configured LLM provider and return the response.
#[tauri::command]
pub async fn athena_chat(state: State<'_, AppState>, message: String) -> Result<String, String> {
    if !state.rate_limiter.check("athena_chat") {
        return Err("Rate limit exceeded. Please wait a moment.".to_string());
    }
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator
        .send_message(message, None)
        .await
        .map_err(|e| e.to_string())
}

/// Send a text message to the LLM provider, associating it with a specific session.
#[tauri::command]
pub async fn athena_chat_with_session(
    state: State<'_, AppState>,
    message: String,
    session_id: String,
) -> Result<String, String> {
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator.set_current_session_id(session_id);
    orchestrator
        .send_message(message, None)
        .await
        .map_err(|e| e.to_string())
}

/// Send a message with image attachments to the LLM provider.
#[tauri::command]
pub async fn athena_chat_with_images(
    state: State<'_, AppState>,
    message: String,
    images: String,
) -> Result<String, String> {
    let image_data: Vec<athena_core::types::ImageData> =
        serde_json::from_str(&images).map_err(|e| e.to_string())?;
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator
        .send_message(message, Some(image_data))
        .await
        .map_err(|e| e.to_string())
}

/// Returns true if `raw_prompt` looks like it contains a secret we must not
/// send to the LLM. Checks plaintext keywords and common l33t-sp34k
/// substitutions (a=@, o=0, e=3, i=1/!, s=$) so trivial obfuscation does not
/// bypass it.
fn prompt_is_sensitive(raw_prompt: &str) -> bool {
    let lowercase = raw_prompt.to_lowercase();
    let plaintext = [
        "password",
        "passw0rd",
        "p@ssword",
        "token",
        "t0ken",
        "t0k3n",
        "secret",
        "s3cret",
        "s3cr3t",
        "api_key",
        "apikey",
        "api-key",
        "api_k3y",
        "authorization",
        "auth",
        "4uth",
        "credential",
        "cr3dential",
        "private key",
        "passphrase",
        "pin",
    ];
    if plaintext.iter().any(|&kw| lowercase.contains(kw)) {
        return true;
    }
    let normalized = lowercase
        .replace('@', "a")
        .replace('0', "o")
        .replace('3', "e")
        .replace('1', "i")
        .replace('!', "i")
        .replace('$', "s");
    let normalized_keywords = [
        "password",
        "token",
        "secret",
        "api_key",
        "apikey",
        "api-key",
        "authorization",
        "auth",
        "credential",
        "private key",
        "passphrase",
        "pin",
    ];
    normalized_keywords.iter().any(|&kw| normalized.contains(kw))
}

/// Summarize a prompt into a short title using the configured LLM.
/// Contract:
/// - `Ok("Sensitive prompt")` — prompt matched the sensitive filter (no LLM call).
/// - `Ok(title)` — the LLM produced a title.
/// - `Err(_)` — missing API key OR retries exhausted. The frontend maps this
///   to a `Failed` title state (empty pill).
#[tauri::command]
pub async fn summarize_agent_title(
    state: State<'_, AppState>,
    raw_prompt: String,
) -> Result<String, String> {
    if prompt_is_sensitive(&raw_prompt) {
        return Ok("Sensitive prompt".to_string());
    }

    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("no api key configured".to_string());
        }
    }
    orchestrator
        .summarize_title(&raw_prompt)
        .await
        .map(|t| t.trim().to_string())
        .map_err(|e| e.to_string())
}

/// Clear all conversation history from the orchestrator.
#[tauri::command]
pub async fn athena_clear_context(state: State<'_, AppState>) -> Result<(), String> {
    let orchestrator = Arc::clone(&state.orchestrator);
    orchestrator.clear_context();
    Ok(())
}

/// Set the conversation history from a list of session entries.
#[tauri::command]
pub async fn athena_set_session_context(
    state: State<'_, AppState>,
    history: String,
) -> Result<(), String> {
    let entries: Vec<athena_core::types::SessionHistoryEntry> =
        serde_json::from_str(&history).map_err(|e| e.to_string())?;
    let orchestrator = Arc::clone(&state.orchestrator);
    orchestrator.set_session_context(entries);
    Ok(())
}

/// Provide an answer to a pending user question from the orchestrator.
#[tauri::command]
pub fn athena_user_answer(
    state: State<'_, AppState>,
    request_id: String,
    answer: String,
) -> Result<bool, String> {
    let mut map = state.pending_questions.lock();
    if let Some(tx) = map.remove(&request_id) {
        let _ = tx.send(answer);
        Ok(true)
    } else {
        log::warn!("no pending question found for request_id: {}", request_id);
        Ok(false)
    }
}

/// Store an API key securely in the OS keychain.
#[tauri::command]
pub fn store_api_key(key: String) -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(&key)
        .map_err(|e| format!("Failed to store API key in keyring: {}", e))
}

/// Clear the API key from the OS keychain.
#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .delete_credential()
        .or_else(|e| {
            if matches!(e, keyring::Error::NoEntry) {
                Ok(())
            } else {
                Err(e)
            }
        })
        .map_err(|e| format!("Failed to clear API key from keyring: {}", e))
}

// ── Output buffer commands ───────────────────────────────────────────────────

/// Append data to an output buffer for a specific pane.
#[tauri::command]
pub fn output_buffer_append(
    state: State<'_, AppState>,
    pane_id: String,
    data: String,
    agent_type: Option<String>,
) {
    state
        .output_buffer
        .append_output(&pane_id, &data, agent_type.as_deref());
}

/// Get output lines from a pane's buffer with optional pagination.
#[tauri::command]
pub fn output_buffer_get(
    state: State<'_, AppState>,
    pane_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}

/// List all agent pane IDs that have captured output.
#[tauri::command]
pub fn output_buffer_list(state: State<'_, AppState>) -> Result<String, String> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

/// Clear the output buffer for a specific pane.
#[tauri::command]
pub fn output_buffer_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, String> {
    Ok(state.output_buffer.clear_pane_buffer(&pane_id))
}

/// Get the accumulated output history for a PTY session.
/// Returns the current grid state as a JSON array of rows with cell characters.
#[tauri::command]
pub fn get_pane_history(state: State<'_, AppState>, pane_id: String) -> Result<String, String> {
    let lines = state.output_buffer.get_output(&pane_id, None);
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}

// ── Output capture commands (aliases matching Electron preload API) ──────────

/// Read captured output from an agent pane (alias for output_buffer_get).
#[tauri::command]
pub fn output_capture_read(
    state: State<'_, AppState>,
    pane_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}

/// List all agent panes with captured output (alias for output_buffer_list).
#[tauri::command]
pub fn output_capture_list_agents(state: State<'_, AppState>) -> Result<String, String> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

/// Get metadata about a pane's output buffer (alias for output_buffer info).
#[tauri::command]
pub fn output_capture_get_info(
    state: State<'_, AppState>,
    pane_id: String,
) -> Result<String, String> {
    match state.output_buffer.get_pane_buffer_info(&pane_id) {
        Some(info) => serde_json::to_string(&info).map_err(|e| e.to_string()),
        None => Ok("null".to_string()),
    }
}

/// Clear an agent pane's captured output (alias for output_buffer_clear).
#[tauri::command]
pub fn output_capture_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, String> {
    Ok(state.output_buffer.clear_pane_buffer(&pane_id))
}

// ── Notification commands ────────────────────────────────────────────────────

/// Push a new notification to the notification service.
#[tauri::command]
pub fn notification_push(
    state: State<'_, AppState>,
    title: String,
    message: String,
    level: Option<String>,
) -> Result<String, String> {
    // Sanitize user-supplied notification content to prevent XSS if content
    // is ever rendered in a context that supports HTML or markdown.
    let title = html_escape(title.trim());
    let message = html_escape(message.trim());
    let notif_type = match level.as_deref() {
        Some("warning") => athena_core::notification::NotificationType::Warning,
        Some("error") => athena_core::notification::NotificationType::Error,
        Some("success") => athena_core::notification::NotificationType::Success,
        Some("needs_input") => athena_core::notification::NotificationType::NeedsInput,
        Some("task_complete") => athena_core::notification::NotificationType::TaskComplete,
        Some("task_error") => athena_core::notification::NotificationType::TaskError,
        _ => athena_core::notification::NotificationType::Info,
    };
    let event = athena_core::notification::NotificationEvent {
        r#type: notif_type,
        title,
        message,
        source: "command".to_string(),
        agent_id: None,
        data: None,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        metadata: None,
        actions: None,
        request_id: None,
    };
    let record = state.notification_service.push_notification(event);
    serde_json::to_string(&record).map_err(|e| e.to_string())
}

/// Get the notification history with optional filtering.
#[tauri::command]
pub fn notification_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::notification::HistoryOptions {
        limit,
        unread_only: None,
        r#type: None,
        source: None,
    };
    let history = state.notification_service.get_history(Some(&options));
    serde_json::to_string(&history).map_err(|e| e.to_string())
}

/// Get the count of unread notifications.
#[tauri::command]
pub fn notification_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.notification_service.get_unread_count())
}

/// Mark a specific notification as read.
#[tauri::command]
pub fn notification_mark_read(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .mark_read(&notification_id)
        .map_err(|e| e.to_string())
}

/// Mark all notifications as read. Returns the number of notifications marked.
#[tauri::command]
pub fn notification_mark_all_read(state: State<'_, AppState>) -> usize {
    state.notification_service.mark_all_read()
}

/// Dismiss (remove) a notification from the history.
#[tauri::command]
pub fn notification_dismiss(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .dismiss(&notification_id)
        .map_err(|e| e.to_string())
}

/// Clear all notifications from the history. Returns the number cleared.
#[tauri::command]
pub fn notification_clear_all(state: State<'_, AppState>) -> usize {
    state.notification_service.clear_all()
}

/// Get a breakdown of notification counts by type.
#[tauri::command]
pub fn notification_counts(state: State<'_, AppState>) -> Result<String, String> {
    let counts = state.notification_service.get_counts();
    serde_json::to_string(&counts).map_err(|e| e.to_string())
}

// ── Plan manager commands ────────────────────────────────────────────────────

/// Create a new execution plan with a goal, reasoning, and steps.
#[tauri::command]
pub fn plan_create(
    state: State<'_, AppState>,
    goal: String,
    reasoning: String,
    steps: String,
) -> Result<String, String> {
    let step_list: Vec<athena_core::plan_manager::PlanStepInput> =
        serde_json::from_str(&steps).map_err(|e| e.to_string())?;
    let input = athena_core::plan_manager::PlanInput {
        goal,
        reasoning,
        steps: step_list,
    };
    let plan = state
        .plan_manager
        .set_active_plan(input)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}

/// Get the currently active plan, if any.
#[tauri::command]
pub fn plan_get(state: State<'_, AppState>) -> Result<String, String> {
    let plan = state.plan_manager.get_active_plan();
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}

/// Update the status of a specific step in the active plan.
#[tauri::command]
pub fn plan_update_step(
    state: State<'_, AppState>,
    step_id: String,
    status: String,
    pane_id: Option<String>,
) -> Result<bool, String> {
    let step_status = match status.as_str() {
        "pending" => athena_core::plan_manager::StepStatus::Pending,
        "in_progress" => athena_core::plan_manager::StepStatus::InProgress,
        "completed" => athena_core::plan_manager::StepStatus::Completed,
        "failed" => athena_core::plan_manager::StepStatus::Failed,
        _ => return Err("Invalid status".to_string()),
    };
    state
        .plan_manager
        .update_step_status(&step_id, step_status, pane_id.as_deref())
        .map_err(|e| e.to_string())
}

// ── Agent comms commands ─────────────────────────────────────────────────────

/// Get the agent comms session token for authenticating agent connections.
///
/// ⚠️ SECURITY: The raw token is NEVER exposed to the frontend. It is only
/// provided to trusted spawned agent processes via environment variables.
/// Corresponding capability `allow-agent-comms-token` has also been removed.
#[tauri::command]
pub fn agent_comms_token() -> Result<String, String> {
    Err("Direct token access is not permitted from the frontend".into())
}

/// Get a list of all active agent sessions.
#[tauri::command]
pub fn agent_comms_sessions(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Send a message to a specific agent via the agent comms channel.
#[tauri::command]
pub fn agent_comms_send(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, String> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| e.to_string())
}

// ── Agents commands (matching Electron preload API naming) ───────────────────

/// List all connected agent sessions (alias for agent_comms_sessions).
#[tauri::command]
pub fn agents_list(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Get the status of a specific agent by its ID.
#[tauri::command]
pub fn agent_get_status(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<String, CommandError> {
    let sessions = state.agent_comms.get_agent_sessions();
    let session = sessions
        .iter()
        .find(|s| s.agent_id == agent_id)
        .ok_or_else(|| CommandError::NotFound(format!("Agent '{}' not found", agent_id)))?;
    serde_json::to_string(&session).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Respond to a pending input request from an agent.
#[tauri::command]
pub fn agent_respond_input(
    state: State<'_, AppState>,
    request_id: String,
    response: String,
) -> Result<bool, String> {
    state
        .agent_comms
        .respond_to_input_request(&request_id, &response)
        .map_err(|e| e.to_string())
}

/// Cancel a pending input request from an agent.
#[tauri::command]
pub fn agent_cancel_input(state: State<'_, AppState>, request_id: String) -> Result<bool, String> {
    state
        .agent_comms
        .cancel_input_request(&request_id)
        .map_err(|e| e.to_string())
}

/// Send a message to a specific agent (alias for agent_comms_send).
#[tauri::command]
pub fn agent_send_message(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, String> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| e.to_string())
}

/// Disconnect an agent by its ID.
#[tauri::command]
pub fn agent_disconnect(state: State<'_, AppState>, agent_id: String) -> Result<bool, String> {
    state
        .agent_comms
        .disconnect_agent(&agent_id)
        .map_err(|e| e.to_string())
}

/// Get the agent comms session token (alias for agent_comms_token).
///
/// ⚠️ SECURITY: The raw token is NEVER exposed to the frontend.
#[tauri::command]
pub fn agent_get_token() -> Result<String, String> {
    Err("Direct token access is not permitted from the frontend".into())
}

// ── Search commands ──────────────────────────────────────────────────────────

/// Search the codebase for a pattern using ripgrep.
#[tauri::command]
pub async fn search_code(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref).map_err(|e| e.to_string())?;
    let options = athena_core::SearchOptions {
        pattern,
        path: validated.to_string_lossy().to_string(),
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Search the codebase using ripgrep (alias for search_code).
#[tauri::command]
pub async fn search_ripgrep(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    search_code(state, pattern, path).await
}

// ── MCP server commands ──────────────────────────────────────────────────────

/// Initialize the MCP server on the given port.
#[tauri::command]
pub async fn mcp_init(state: State<'_, AppState>, port: u16) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    server.init(port).map_err(|e| e.to_string())
}

/// Shut down the MCP server.
#[tauri::command]
pub async fn mcp_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    server.shutdown();
    Ok(())
}

/// Handle a JSON-RPC request through the MCP server.
#[tauri::command]
pub async fn mcp_handle_request(
    state: State<'_, AppState>,
    request: String,
) -> Result<String, String> {
    if request.len() > caps::MAX_REQUEST_BYTES {
        return Err(format!(
            "request too large: {} > {}",
            request.len(),
            caps::MAX_REQUEST_BYTES
        ));
    }
    let req =
        athena_core::mcp::McpServer::parse_request(&request).ok_or("Invalid JSON-RPC request")?;

    // Take the server lock, then bound how long we hold it across the
    // (potentially slow, tool-executing) handle_request call. Without a
    // timeout, a single hung tool call would block every other MCP IPC call
    // for as long as the tool runs. The lock is a tokio::sync::Mutex (which
    // yields correctly, so this does not stall the runtime) but it still
    // serializes all MCP requests; the timeout bounds that serialization so
    // a wedged request can't pin the queue indefinitely.
    let server = state.mcp_server.lock().await;
    let resp = match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        server.handle_request(&req),
    )
    .await
    {
        Ok(resp) => resp,
        Err(_) => {
            log::warn!(
                "MCP handle_request timed out after 60s for method {}",
                req.method
            );
            athena_core::mcp::JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: None,
                error: Some(athena_core::mcp::JsonRpcError {
                    code: -32603,
                    message: "request timed out".into(),
                    data: None,
                }),
            }
        }
    };
    Ok(athena_core::mcp::McpServer::serialize_response(&resp))
}

/// Broadcast a notification to all connected MCP clients.
#[tauri::command]
pub async fn mcp_broadcast(
    state: State<'_, AppState>,
    method: String,
    params: String,
) -> Result<(), String> {
    let server = state.mcp_server.lock().await;
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    server.broadcast_notification(&method, &params_json);
    Ok(())
}

/// List all tools exposed by the MCP server.
#[tauri::command]
pub fn mcp_tools() -> Result<String, String> {
    let tools = athena_core::mcp::get_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}

// ── Swarm commands ───────────────────────────────────────────────────────────

/// Read the current swarm state from the given directory.
#[tauri::command]
pub async fn swarm_read_state(state: State<'_, AppState>, dir: String) -> Result<String, String> {
    let dir_path = std::path::Path::new(&dir);
    let _ = validate_path_exists(&state.store, dir_path).map_err(|e| e.to_string())?;
    let coordinator = state.swarm_coordinator.lock().await;
    let result = coordinator
        .read_state(&dir)
        .await
        .map_err(|e| e.to_string())?;
    match result {
        Some(s) => serde_json::to_string(&s).map_err(|e| e.to_string()),
        None => Ok("null".to_string()),
    }
}

/// Send a message from one swarm agent to another via the mailbox system.
#[tauri::command]
pub async fn swarm_send_message(
    state: State<'_, AppState>,
    dir: String,
    from: String,
    to: String,
    content: String,
) -> Result<(), String> {
    let dir_path = std::path::Path::new(&dir);
    let _ = validate_path_exists(&state.store, dir_path).map_err(|e| e.to_string())?;
    let coordinator = state.swarm_coordinator.lock().await;
    coordinator
        .send_message(&dir, &from, &to, &content)
        .await
        .map_err(|e| e.to_string())
}

/// Read all messages from a swarm agent's mailbox.
#[tauri::command]
pub async fn swarm_read_mailbox(
    state: State<'_, AppState>,
    dir: String,
    agent_id: String,
) -> Result<String, String> {
    let dir_path = std::path::Path::new(&dir);
    let _ = validate_path_exists(&state.store, dir_path).map_err(|e| e.to_string())?;
    let coordinator = state.swarm_coordinator.lock().await;
    let messages = coordinator
        .read_mailbox(&dir, &agent_id)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&messages).map_err(|e| e.to_string())
}

// ── Shell integration commands ───────────────────────────────────────────────

/// Parse OSC 633 sequences from terminal output data.
#[tauri::command]
pub async fn shell_integration_parse(
    state: State<'_, AppState>,
    data: String,
) -> Result<String, String> {
    if data.len() > caps::MAX_DATA_BYTES {
        return Err(format!(
            "data too large: {} > {}",
            data.len(),
            caps::MAX_DATA_BYTES
        ));
    }
    let shell_integration_parser = state.shell_integration_parser.clone();
    tokio::task::spawn_blocking(move || {
        let mut parser = shell_integration_parser.lock();
        let sequences = parser.feed(&data);
        serde_json::to_string(&sequences).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get the shell integration script for the specified shell (bash, zsh, fish).
///
/// Returns an error if the shell is not one of the supported shells. Callers
/// should not inject a fallback script for unsupported shells — the script
/// syntax is shell-specific and a mismatched injection will break the shell.
#[tauri::command]
pub fn shell_integration_script(shell: String) -> Result<String, CommandError> {
    athena_core::shell_integration::get_shell_integration_script(&shell)
        .map_err(|e| CommandError::InvalidInput(e.to_string()))
}

/// Check whether the specified shell supports shell integration.
#[tauri::command]
pub fn shell_integration_compatible(shell: String) -> bool {
    athena_core::shell_integration::is_shell_integration_compatible(&shell)
}

/// Strip OSC 633 sequences from terminal output data.
#[tauri::command]
pub fn shell_integration_strip(data: String) -> String {
    athena_core::shell_integration::strip_osc633(&data)
}

// ── Tool executor commands ───────────────────────────────────────────────────

/// Execute a built-in tool by name with the given arguments.
///
/// Only a whitelist of safe tools may be invoked from the frontend to prevent
/// arbitrary tool abuse. Destructive tools (shell execution, terminal control,
/// file deletion, etc.) are rejected at the command boundary.
#[tauri::command]
pub async fn tool_execute(
    state: State<'_, AppState>,
    tool_name: String,
    arguments: String,
) -> Result<String, String> {
    const ALLOWED: &[&str] = &[
        "read_agent_output",
        "list_agents",
        "check_agent_status",
        "kanban_list_tasks",
        "fs_read_file",
        "fs_list_dir",
        "fs_search",
    ];
    if !ALLOWED.contains(&tool_name.as_str()) {
        return Err(format!(
            "Tool execution denied: '{}' is not in the allowed frontend tool whitelist",
            tool_name
        ));
    }
    let tool_executor = state.tool_executor.clone();
    tokio::task::spawn_blocking(move || {
        let executor = tool_executor.lock();
        let input: athena_core::tool_executor::ToolInput =
            serde_json::from_str(&arguments).map_err(|e| e.to_string())?;
        let result = executor
            .execute_tool_call(&tool_name, &input)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List all built-in tools available in the tool executor.
#[tauri::command]
pub fn tool_list() -> Result<String, String> {
    let tools = athena_core::tool_executor::orchestrator_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}

/// Get the OpenAI-compatible tool schemas for all built-in tools.
#[tauri::command]
pub fn tool_openai_schema() -> Result<String, String> {
    let schemas = athena_core::tool_executor::to_openai_tools();
    serde_json::to_string(&schemas).map_err(|e| e.to_string())
}

// ── Browser commands (child webview) ─────────────────────────────────────────

fn get_normalized_url(url: &str) -> Result<String, String> {
    athena_browser::normalize_url(url).map_err(|e| e.to_string())
}

/// Child webview label for a given browser panel id.
fn child_label(id: &str) -> String {
    format!("browser-child-{}", id)
}

/// Find the main window. In Tauri 2.0, the default window label is "main".
fn main_window(state: &AppState) -> Result<tauri::Window, String> {
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    handle
        .get_window("main")
        .ok_or_else(|| "Main window not found".to_string())
}

/// Calculate default position and size for the right sidebar browser child webview.
fn sidebar_bounds(
    window: &tauri::Window,
) -> Result<
    (
        tauri_runtime::dpi::LogicalPosition<f64>,
        tauri_runtime::dpi::LogicalSize<f64>,
    ),
    String,
> {
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let sidebar_w = 420u32;
    let x = size.width.saturating_sub(sidebar_w).saturating_sub(15) as f64;
    let y = 120.0f64; // below header/toolbar
    let w = sidebar_w as f64;
    let h = (size.height.saturating_sub(120).saturating_sub(60)) as f64;
    Ok((
        tauri_runtime::dpi::LogicalPosition::new(x, y),
        tauri_runtime::dpi::LogicalSize::new(w, h),
    ))
}

/// Open (show) a browser panel — creates the child webview if not already present.
#[tauri::command]
pub fn browser_show(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    let normalized = get_normalized_url(&url)?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if handle.get_webview(&label).is_none() {
        let w = main_window(&state)?;
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        let builder = tauri::WebviewBuilder::new(&label, tauri::WebviewUrl::External(parsed));
        let (pos, sz) = sidebar_bounds(&w)?;
        w.add_child(builder, pos, sz).map_err(|e| e.to_string())?;
        {
            let mut labels = state.child_webview_labels.lock();
            labels.insert(label);
        }
    }

    state
        .browser_manager
        .open_browser(&id, &normalized)
        .map_err(|e| e.to_string())
}

/// Hide (close) a browser panel — destroys the child webview.
#[tauri::command]
pub fn browser_hide(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let _ = webview.close();
    }
    {
        let mut labels = state.child_webview_labels.lock();
        labels.remove(&label);
    }

    state
        .browser_manager
        .close_browser(&id)
        .map_err(|e| e.to_string())
}

/// Navigate a browser panel to a new URL.
#[tauri::command]
pub fn browser_navigate(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    let normalized = get_normalized_url(&url)?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    } else {
        let w = main_window(&state)?;
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        let builder = tauri::WebviewBuilder::new(&label, tauri::WebviewUrl::External(parsed));
        let (pos, sz) = sidebar_bounds(&w)?;
        w.add_child(builder, pos, sz).map_err(|e| e.to_string())?;
        {
            let mut labels = state.child_webview_labels.lock();
            labels.insert(label);
        }
    }

    state
        .browser_manager
        .navigate(&id, &normalized)
        .map_err(|e| e.to_string())
}

/// Navigate back in browser history.
#[tauri::command]
pub fn browser_back(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let url = state
        .browser_manager
        .go_back(&id)
        .map_err(|e| e.to_string())?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    }

    Ok(url)
}

/// Navigate forward in browser history.
#[tauri::command]
pub fn browser_forward(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let url = state
        .browser_manager
        .go_forward(&id)
        .map_err(|e| e.to_string())?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    }

    Ok(url)
}

/// Reload the current browser page.
#[tauri::command]
pub fn browser_reload(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        webview.reload().map_err(|e| e.to_string())?;
    }

    state.browser_manager.reload(&id).map_err(|e| e.to_string())
}

/// Reposition/resize the browser child webview to match a frontend-measured rect.
///
/// The frontend owns a placeholder `<div>` and reports its on-screen bounds (in
/// logical pixels) so the native child webview tracks the resizable sidebar, the
/// main-area panel, and window resizes. No-op if the webview doesn't exist yet.
/// Passing off-screen coordinates "parks" the webview (keeps the page alive while
/// hidden) when its surface unmounts.
#[tauri::command]
pub fn browser_set_bounds(
    state: State<'_, AppState>,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        webview
            .set_position(tauri_runtime::dpi::LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(tauri_runtime::dpi::LogicalSize::new(
                width.max(0.0),
                height.max(0.0),
            ))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
// ── Plugin commands ──────────────────────────────────────────────────────────

/// List all registered plugins.
#[tauri::command]
pub fn plugin_list(state: State<'_, AppState>) -> Result<String, String> {
    let plugins = state.plugin_manager.list_plugins();
    serde_json::to_string(&plugins).map_err(|e| e.to_string())
}

/// Get detailed information about a specific plugin.
#[tauri::command]
pub fn plugin_get(state: State<'_, AppState>, plugin_id: String) -> Result<String, CommandError> {
    let plugin = state
        .plugin_manager
        .get_plugin_info(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&plugin).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Register a new plugin with the plugin manager.
#[tauri::command]
pub fn plugin_register(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Unregister a plugin by its ID.
#[tauri::command]
pub fn plugin_unregister(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Enable a plugin by its ID.
#[tauri::command]
pub fn plugin_enable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .enable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Disable a plugin by its ID.
#[tauri::command]
pub fn plugin_disable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .disable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Get the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_get_config(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<String, CommandError> {
    let config = state
        .plugin_manager
        .get_plugin_config(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&config).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Set the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_set_config(
    state: State<'_, AppState>,
    plugin_id: String,
    config: String,
) -> Result<(), String> {
    let config_value: serde_json::Value =
        serde_json::from_str(&config).map_err(|e| e.to_string())?;
    state
        .plugin_manager
        .set_plugin_config(&plugin_id, &config_value)
        .map_err(|e| e.to_string())
}

/// Record an error for a specific plugin.
#[tauri::command]
pub fn plugin_set_error(
    state: State<'_, AppState>,
    plugin_id: String,
    error: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .set_plugin_error(&plugin_id, &error)
        .map_err(|e| e.to_string())
}

// ── Plugin host commands ─────────────────────────────────────────────────────

/// List all active plugin host sessions.
#[tauri::command]
pub fn plugin_host_list_sessions(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.plugin_manager.list_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Get details about a specific plugin host session.
#[tauri::command]
pub fn plugin_host_get_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, CommandError> {
    let session = state
        .plugin_manager
        .get_session(&session_id)
        .ok_or_else(|| CommandError::NotFound(format!("Session '{}' not found", session_id)))?;
    serde_json::to_string(&session).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Emit a plugin event with the given type and data.
#[tauri::command]
pub fn plugin_host_emit_event(
    state: State<'_, AppState>,
    event_type: String,
    data: String,
) -> Result<String, String> {
    let parsed_type: athena_plugins::PluginEventType =
        serde_json::from_str(&format!("\"{}\"", event_type.to_lowercase()))
            .map_err(|e| format!("Invalid event type '{}': {}", event_type, e))?;
    let payload: athena_plugins::PluginEventPayload =
        serde_json::from_str(&data).map_err(|e| format!("Invalid event payload: {}", e))?;
    let source = athena_plugins::PluginEventSource {
        session_id: String::new(),
        pane_id: None,
        agent_type: String::new(),
        agent_id: None,
    };
    let event = state
        .plugin_manager
        .emit_plugin_event(parsed_type, source, payload);
    serde_json::to_string(&event).map_err(|e| e.to_string())
}

/// Subscribe a session to specific plugin event types.
#[tauri::command]
pub fn plugin_host_subscribe(
    state: State<'_, AppState>,
    session_id: String,
    event_types: String,
) -> Result<(), String> {
    let types: Vec<athena_plugins::PluginEventType> =
        serde_json::from_str(&event_types).map_err(|e| format!("Invalid event types: {}", e))?;
    state
        .plugin_manager
        .subscribe_session(&session_id, &types)
        .map_err(|e| e.to_string())
}

/// Update the status of a plugin host session.
#[tauri::command]
pub fn plugin_host_update_status(
    state: State<'_, AppState>,
    session_id: String,
    status: String,
) -> Result<(), String> {
    let parsed_status: athena_plugins::SessionStatus =
        serde_json::from_str(&format!("\"{}\"", status.to_lowercase()))
            .map_err(|e| format!("Invalid session status '{}': {}", status, e))?;
    state
        .plugin_manager
        .update_session_status(&session_id, parsed_status, None)
        .map_err(|e| e.to_string())
}

/// Unregister a plugin host session.
#[tauri::command]
pub fn plugin_host_unregister_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .remove_session(&session_id)
        .map_err(|e| e.to_string())
}

/// Discover plugins in the given directory by scanning for manifest files.
#[tauri::command]
pub fn plugin_host_discover_plugins(
    state: State<'_, AppState>,
    dir: String,
) -> Result<String, String> {
    // Validate the plugin directory is within the workspace before scanning.
    let _ = validate_path_exists(&state.store, std::path::Path::new(&dir))
        .map_err(|e| e.to_string())?;
    let results = state
        .plugin_manager
        .discover_plugins(std::path::Path::new(&dir))
        .map_err(|e| e.to_string())?;
    // Convert inner errors to strings since PluginError doesn't implement Serialize
    let serializable: Vec<serde_json::Value> = results
        .into_iter()
        .map(|r| match r {
            Ok(manifest) => serde_json::to_value(manifest)
                .unwrap_or_else(|_| serde_json::json!({"error": "serialization failed"})),
            Err(e) => serde_json::json!({"error": e.to_string()}),
        })
        .collect();
    serde_json::to_string(&serializable).map_err(|e| e.to_string())
}

/// Register and set up a plugin with the given manifest information.
#[tauri::command]
pub fn plugin_host_setup_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Remove a plugin by its ID (alias for plugin_unregister).
#[tauri::command]
pub fn plugin_host_remove_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// App-exit resume capture
// ---------------------------------------------------------------------------

/// Merge captured `pane_id -> resume_id` pairs directly into the persisted
/// `workspaces` JSON — the single source of truth the frontend loads on
/// startup. For each matching pane this sets `resume_id`, clears `resume_cmd`,
/// and resets `resume_dismissed = false` so the resume banner reappears on the
/// next launch via the normal workspace-load path (no separate transient key
/// for the frontend to reconcile, and no frontend startup changes).
///
/// Operates on `serde_json::Value` to avoid coupling the backend to the
/// frontend's `WorkspaceState`/`PaneConfig` Rust types. Returns the number of
/// panes updated. A missing/empty `workspaces` key (first run) yields `Ok(0)`.
fn merge_resume_ids_into_workspaces(
    store: &athena_store::KeyValueStore,
    ids: &std::collections::HashMap<String, String>,
    cmds: &std::collections::HashMap<String, String>,
) -> Result<usize, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    let json = match store.get::<String>("workspaces") {
        Ok(Some(j)) if !j.trim().is_empty() => j,
        Ok(_) => return Ok(0), // no workspace persisted yet — nothing to merge into
        Err(e) => return Err(e.to_string()),
    };
    let mut root: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    let mut updated = 0usize;
    if let Some(spaces) = root.get_mut("spaces").and_then(|v| v.as_array_mut()) {
        for space in spaces.iter_mut() {
            let Some(panes) = space.get_mut("panes").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for pane in panes.iter_mut() {
                let pane_id = pane
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let Some(pane_id) = pane_id else { continue };
                let Some(resume_id) = ids.get(&pane_id) else {
                    continue;
                };
                if let Some(obj) = pane.as_object_mut() {
                    obj.insert(
                        "resume_id".into(),
                        serde_json::Value::String(resume_id.clone()),
                    );
                    // Prefer a captured resume_cmd ; fall back to one built from
                    // the resume_id so Shell panes (whose agent_type can't be
                    // synthesized) still get a displayable command.
                    let resume_cmd = cmds
                        .get(&pane_id)
                        .cloned()
                        .unwrap_or_else(|| resume_id.clone());
                    obj.insert("resume_cmd".into(), serde_json::Value::String(resume_cmd));
                    obj.insert("resume_dismissed".into(), serde_json::Value::Bool(false));
                    updated += 1;
                }
            }
        }
    }

    if updated > 0 {
        let out = serde_json::to_string(&root).map_err(|e| e.to_string())?;
        store
            .set_sync("workspaces", &out)
            .map_err(|e| e.to_string())?;
    }
    Ok(updated)
}

/// App-exit resume capture, invoked from `RunEvent::Exit` (the event macOS
/// Cmd+Q reliably fires). Types `/exit` into every live PTY so agents (Claude,
/// Codex, …) exit gracefully and print their `<cli> --resume <id>` line — the
/// same line the live frontend scanner catches during a manual `/exit`. We then
/// scan each pane's output buffer for that id and merge it straight into the
/// persisted `workspaces` state, so the banner reappears on next launch. Plain
/// shells just echo a harmless "not found" and yield no match.
///
/// Returns the number of panes whose resume id was captured + persisted.
///
/// Concurrency: the caller runs this on a DEDICATED runtime/thread during
/// `RunEvent::Exit`, while the shared runtime's `pty_read_loop` tasks keep
/// feeding the output buffer with the agents' exit output (see
/// `capture_resume_on_exit` in main.rs).
pub async fn capture_resume_ids_on_exit(state: &AppState, wait_ms: u64) -> usize {
    let all_sessions = {
        let sm = state.session_manager.lock().await;
        sm.list_sessions().await
    };
    if all_sessions.is_empty() {
        return 0;
    }

    // Classify each session's foreground process so we only nudge *agents*
    // with `/exit`. Plain shells never produce a resume id, so sending to
    // them would waste the entire wait budget. The classification costs a `ps`
    // per session, but that is fast compared to the 4 s wait budget.
    let agent_sessions: Vec<String> = {
        let sm = state.session_manager.lock().await;
        let mut agents = Vec::new();
        for id in &all_sessions {
            match sm.get_session(id).await {
                Some(s) => {
                    let label = session_foreground_label(&s).await;
                    if AGENT_FG_NAMES.contains(&label.as_str()) {
                        agents.push(id.clone());
                    }
                }
                None => continue,
            }
        }
        agents
    };

    if agent_sessions.is_empty() {
        log::info!(
            "capture_resume_ids_on_exit: {} live session(s), none are agents — nothing to do",
            all_sessions.len()
        );
        return 0;
    }

    log::info!(
        "capture_resume_ids_on_exit: {} live session(s), {} agent(s) — nudging with /exit",
        all_sessions.len(),
        agent_sessions.len()
    );

    // Send `/exit` + Enter to every agent PTY.
    {
        let sm = state.session_manager.lock().await;
        for id in &agent_sessions {
            if let Err(e) = sm.write(id, b"/exit\r").await {
                log::warn!("capture_resume_ids_on_exit: write to {} failed: {}", id, e);
            }
        }
    }

    // Poll the output buffer until every agent pane yields a resume id or we
    // hit the deadline. The PTY read loops populate the buffer on the shared
    // runtime.
    let step_ms = 150u64;
    let mut elapsed = 0u64;
    let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut found_cmds: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    while elapsed < wait_ms && found.len() < agent_sessions.len() {
        tokio::time::sleep(std::time::Duration::from_millis(step_ms)).await;
        elapsed += step_ms;
        for id in &agent_sessions {
            if found.contains_key(id) {
                continue;
            }
            let lines = state.output_buffer.get_output(id, None);
            if lines.is_empty() {
                continue;
            }
            let text: String = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if let Some((prefix, rid)) = athena_core::resume_scanner::scan_text_for_resume_id(&text)
            {
                log::info!(
                    "capture_resume_ids_on_exit: captured resume id for pane {}",
                    id
                );
                let cmd = format!("{} {}", prefix, rid);
                found_cmds.insert(id.clone(), cmd);
                found.insert(id.clone(), rid);
            }
        }
    }

    if found.is_empty() {
        log::info!("capture_resume_ids_on_exit: no resume ids captured");
        return 0;
    }

    match merge_resume_ids_into_workspaces(&state.store, &found, &found_cmds) {
        Ok(n) => {
            if let Err(e) = state.store.flush_if_dirty().await {
                log::error!("capture_resume_ids_on_exit: KV flush failed: {}", e);
            }
            log::info!(
                "capture_resume_ids_on_exit: merged {} resume id(s) into {} pane(s)",
                found.len(),
                n
            );
            n
        }
        Err(e) => {
            log::error!(
                "capture_resume_ids_on_exit: merge into workspaces failed: {}",
                e
            );
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // is_within_any_root is pure logic over canonicalized paths, so we can
    // exercise it without touching the disk (other than via canonicalize of
    // the temp dirs we construct).

    #[test]
    fn is_within_any_root_accepts_descendant_of_a_trusted_root() {
        let temp = std::env::temp_dir();
        let sub = temp.join("athena_trusted_descendant");
        std::fs::create_dir_all(&sub).unwrap();
        let canon = sub.canonicalize().unwrap();
        let roots = vec![temp.canonicalize().unwrap()];
        assert!(is_within_any_root(&canon, &roots));
        std::fs::remove_dir_all(&sub).ok();
    }

    #[test]
    fn is_within_any_root_rejects_sibling_outside_all_roots() {
        // Two roots; a path under neither must be rejected.
        let temp = std::env::temp_dir();
        let a = temp.join("athena_tr_root_a");
        let b = temp.join("athena_tr_root_b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let canon_a = a.canonicalize().unwrap();
        let canon_b = b.canonicalize().unwrap();
        // b is NOT under a's tree
        let roots = vec![canon_a.clone()];
        assert!(!is_within_any_root(&canon_b, &roots));
        // but is accepted once b is itself a root
        let roots = vec![canon_a, canon_b.clone()];
        assert!(is_within_any_root(&canon_b, &roots));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn load_trusted_roots_recanonicalizes_and_skips_missing() {
        let store = athena_store::KeyValueStore::new_empty();

        // A real, canonicalizable path round-trips; a missing/garbage entry
        // is skipped without error.
        let temp = std::env::temp_dir().join("athena_tr_load_real");
        std::fs::create_dir_all(&temp).unwrap();
        let canon = temp.canonicalize().unwrap().to_string_lossy().into_owned();
        store
            .set_sync(
                TRUSTED_ROOTS_KEY,
                &vec![
                    canon.clone(),
                    "/this/path/does/not/exist/athena".to_string(),
                ],
            )
            .unwrap();
        let roots = load_trusted_roots(&store);
        // only the real one survives
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], std::path::PathBuf::from(&canon));

        // Missing key -> empty list, not an error.
        let store2 = athena_store::KeyValueStore::new_empty();
        assert!(load_trusted_roots(&store2).is_empty());

        // Malformed value -> empty list, not an error.
        let store3 = athena_store::KeyValueStore::new_empty();
        store3.set_sync(TRUSTED_ROOTS_KEY, &"not-a-json-array").ok();
        assert!(load_trusted_roots(&store3).is_empty());

        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn validate_path_exists_accepts_trusted_root_outside_project() {
        // Simulates the bug report: a Space dir outside the app's project root
        // must be accepted once trusted, and rejected before it's trusted.
        let store = athena_store::KeyValueStore::new_empty();
        let temp = std::env::temp_dir().join("athena_tr_outside_project");
        std::fs::create_dir_all(&temp).unwrap();
        let canon = temp.canonicalize().unwrap();

        // Before trusting: the result must match "is canon under the project
        // root" — robust to where the test physically runs.
        let pre = validate_path_exists(&store, &canon);
        let project_root = get_workspace_root().ok();
        let expected_ok = project_root
            .as_ref()
            .map(|r| canon.starts_with(r))
            .unwrap_or(false);
        assert_eq!(pre.is_ok(), expected_ok);

        // After trusting: accepted.
        let mut roots = load_trusted_roots(&store);
        roots.push(canon.clone());
        let strs: Vec<String> = roots
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        store.set_sync(TRUSTED_ROOTS_KEY, &strs).unwrap();
        assert!(validate_path_exists(&store, &canon).is_ok());

        std::fs::remove_dir_all(&temp).ok();
    }

    fn ids(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn merge_resume_ids_sets_resume_fields_on_matching_pane() {
        let store = athena_store::KeyValueStore::new_empty();
        // A persisted workspace whose pane previously had its banner dismissed.
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({
                    "spaces": [{
                        "id": "space-1",
                        "panes": [
                            { "id": "pane-a", "resume_id": "old", "resume_dismissed": true },
                            { "id": "pane-b" }
                        ]
                    }],
                    "active_space_id": "space-1"
                })
                .to_string(),
            )
            .unwrap();

        let updated = merge_resume_ids_into_workspaces(
            &store,
            &ids(&[("pane-a", "new-resume-id")]),
            &ids(&[]),
        )
        .unwrap();
        assert_eq!(updated, 1);

        let json: String = store.get("workspaces").unwrap().unwrap();
        let root: serde_json::Value = serde_json::from_str(&json).unwrap();
        let pane_a = &root["spaces"][0]["panes"][0];
        assert_eq!(pane_a["resume_id"], "new-resume-id");
        assert_eq!(pane_a["resume_dismissed"], false);
        // When no explicit resume_cmd is captured, the fallback is the
        // resume_id itself (so Shell panes can show the banner too).
        assert_eq!(pane_a["resume_cmd"], "new-resume-id");
        // Untouched pane keeps its shape (no resume id forced on it).
        let pane_b = &root["spaces"][0]["panes"][1];
        assert!(pane_b.get("resume_id").is_none());
    }

    #[test]
    fn merge_resume_ids_is_noop_when_no_pane_matches() {
        let store = athena_store::KeyValueStore::new_empty();
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({
                    "spaces": [{ "id": "s", "panes": [{ "id": "pane-x" }] }],
                    "active_space_id": "s"
                })
                .to_string(),
            )
            .unwrap();

        let updated =
            merge_resume_ids_into_workspaces(&store, &ids(&[("pane-unknown", "rid")]), &ids(&[]))
                .unwrap();
        assert_eq!(updated, 0);
    }

    #[test]
    fn merge_resume_ids_handles_missing_or_empty_workspaces_key() {
        let store = athena_store::KeyValueStore::new_empty();
        // Missing key.
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[("p", "r")]), &ids(&[])).unwrap(),
            0
        );
        // Empty string value.
        store.set_sync("workspaces", &"").unwrap();
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[("p", "r")]), &ids(&[])).unwrap(),
            0
        );
        // Empty ids map is a no-op even with a real workspace.
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({ "spaces": [], "active_space_id": null }).to_string(),
            )
            .unwrap();
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[]), &ids(&[])).unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod title_command_tests {
    use super::prompt_is_sensitive;

    #[test]
    fn sensitive_prompt_blocks_plaintext_variants() {
        let cases = [
            "my password is x",
            "set the API_KEY=..",
            "a secret token",
            "auth header here",
            "credential leak",
        ];
        for kw in cases {
            assert!(prompt_is_sensitive(kw), "expected sensitive: {kw}");
        }
    }

    #[test]
    fn sensitive_prompt_blocks_l33t_variants() {
        let cases = ["p@ssword", "t0k3n", "API_K3Y", "s3cret"];
        for kw in cases {
            assert!(prompt_is_sensitive(kw), "expected l33t-sensitive: {kw}");
        }
    }

    #[test]
    fn normal_prompt_passes_filter() {
        assert!(!prompt_is_sensitive("analyze the codebase"));
        assert!(!prompt_is_sensitive("what rust version is this"));
        assert!(!prompt_is_sensitive("hi"));
    }
}
