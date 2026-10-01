//! Project-convention files (AGENTS.md, CLAUDE.md, .cursorrules,
//! .goosehints) discovered from a workspace root upward to the user's home
//! directory (never beyond it) and injected into chat/swarm contexts.

use std::path::Path;

/// Files we auto-detect, in discovery order.
pub const CONTEXT_FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".cursorrules", ".goosehints"];

/// Per-file read cap; anything past the cap is truncated with a marker (a
/// 200 KB AGENTS.md should never silently eat a model's context window).
pub const MAX_FILE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFile {
    pub path: String,
    pub name: String,
    pub bytes: u64,
}

/// Discover convention files from `dir` upward to `$HOME` (inclusive),
/// workspace-first order. Parents above home are never consulted — a stray
/// `/AGENTS.md` on a server is none of our business.
pub fn discover(dir: &Path) -> Vec<ContextFile> {
    let mut out = Vec::new();
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let mut current = match dir.canonicalize() {
        Ok(d) => Some(d),
        Err(_) => return out,
    };
    while let Some(cur) = current {
        for name in CONTEXT_FILENAMES {
            let candidate = cur.join(name);
            if candidate.is_file() {
                let bytes = candidate
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(0);
                out.push(ContextFile {
                    path: candidate.display().to_string(),
                    name: name.to_string(),
                    bytes,
                });
            }
        }
        if home.as_ref().is_some_and(|h| &cur == h) {
            break;
        }
        match cur.parent() {
            Some(p) => current = Some(p.to_path_buf()),
            None => break,
        }
    }
    out
}

/// Read and concatenate the given context files (capped per file), each with
/// a `## <path>` header so the model can attribute rules to their owner.
pub fn read_concat(paths: &[&Path]) -> String {
    let mut out = String::new();
    for path in paths {
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.len() > MAX_FILE_BYTES as u64 * 4 {
            // Absurdly large: likely not a text doc; skip entirely.
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format!("## {}\n\n", path.display()));
        if content.len() > MAX_FILE_BYTES {
            out.push_str(&content[..MAX_FILE_BYTES]);
            out.push_str("\n\n… (truncated)");
        } else {
            out.push_str(&content);
        }
    }
    out
}

/// Copy each context file into the swarm worktree `worktree` under its
/// basename. Existing files win (a committed CLAUDE.md owns itself) — the
/// helper never overwrites. Returns the number of files written.
pub fn copy_into_worktree(worktree: &Path, files: &[&Path]) -> Result<usize, String> {
    let mut written = 0usize;
    for file in files {
        let Some(name) = file.file_name() else { continue };
        let dest = worktree.join(name);
        if dest.exists() {
            continue;
        }
        let content = std::fs::read(file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        std::fs::write(&dest, content)
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(std::path::PathBuf);
    impl Tmp {
        fn new() -> Self {
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let d = std::env::temp_dir().join(format!(
                "athena-ctx-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&d).unwrap();
            Tmp(d)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn discovers_nearest_convention_files() {
        let tmp = Tmp::new();
        let proj = tmp.0.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(tmp.0.join("AGENTS.md"), "root rules").unwrap();
        std::fs::write(proj.join("CLAUDE.md"), "project rules").unwrap();

        let found = discover(&proj);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert!(names.iter().any(|n| n.ends_with("CLAUDE.md")), "{names:?}");
        assert!(names.iter().any(|n| n.ends_with("AGENTS.md")), "{names:?}");
    }

    #[test]
    fn read_concat_caps_and_headers() {
        let tmp = Tmp::new();
        let big = tmp.0.join("big.md");
        std::fs::write(&big, "x".repeat(MAX_FILE_BYTES + 10)).unwrap();
        let small = tmp.0.join("small.md");
        std::fs::write(&small, "ok").unwrap();

        let text = read_concat(&[&big, &small]);
        assert!(text.contains("## "));
        assert!(text.contains("… (truncated)"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn copy_into_worktree_respects_existing() {
        let tmp = Tmp::new();
        let src = tmp.0.join("AGENTS.md");
        std::fs::write(&src, "parent rules").unwrap();
        let wt = tmp.0.join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        // Existing worktree file wins.
        std::fs::write(wt.join("AGENTS.md"), "repo rules").unwrap();

        let written = copy_into_worktree(&wt, &[&src]).unwrap();
        assert_eq!(written, 0);
        assert_eq!(std::fs::read_to_string(wt.join("AGENTS.md")).unwrap(), "repo rules");

        std::fs::remove_file(wt.join("AGENTS.md")).unwrap();
        let written = copy_into_worktree(&wt, &[&src]).unwrap();
        assert_eq!(written, 1);
        assert_eq!(std::fs::read_to_string(wt.join("AGENTS.md")).unwrap(), "parent rules");
    }
}
