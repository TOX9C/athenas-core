//! Local git repository inspection for Athena's Core.
//!
//! Read-only operations (discover / status / diff) over `libgit2`. Transport
//! features are disabled at the crate level: these functions only ever touch
//! on-disk repositories.

use std::path::{Path, PathBuf};

/// Hard cap on accumulated diff text returned by [`diff`]; past this the
/// result is marked `truncated` instead of growing without bound.
pub const MAX_DIFF_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git operation failed: {0}")]
    Git(String),
    #[error("not a git repository: {0}")]
    NotARepo(String),
}

impl From<git2::Error> for GitError {
    fn from(e: git2::Error) -> Self {
        GitError::Git(e.message().to_string())
    }
}

/// Simplified per-side (index / workdir) status of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    New,
    Modified,
    Deleted,
    TypeChanged,
    Renamed,
    Conflicted,
}

/// Status of one file; `index` is the staged state, `workdir` the unstaged
/// state. Untracked files report `workdir: New` with `index: None`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileStatus {
    pub path: String,
    pub index: Option<StatusKind>,
    pub workdir: Option<StatusKind>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoStatus {
    pub root: String,
    pub branch: Option<String>,
    pub files: Vec<FileStatus>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffLine {
    /// '+' addition, '-' deletion, ' ' context (matches unified diff origin).
    pub origin: char,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffSet {
    pub files: Vec<FileDiff>,
    pub truncated: bool,
}

/// Walk upward from `path` to find an enclosing git repository; returns the
/// repository workdir root, or `None` outside any repo.
pub fn discover_repo(path: &Path) -> Result<Option<PathBuf>, GitError> {
    match git2::Repository::discover(path) {
        Ok(repo) => Ok(repo
            .workdir()
            .or_else(|| repo.path().parent())
            .map(|p| canonicalize(p.to_path_buf()))),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn canonicalize(p: PathBuf) -> PathBuf {
    p.canonicalize().unwrap_or(p)
}

fn open(path: &Path) -> Result<git2::Repository, GitError> {
    git2::Repository::discover(path)
        .map_err(|_| GitError::NotARepo(path.display().to_string()))
}

/// Snapshot of the repository containing `path`: current branch and the
/// staged (`index`) + unstaged (`workdir`) state of every changed file.
pub fn status(path: &Path) -> Result<RepoStatus, GitError> {
    let repo = open(path)?;
    let root = repo
        .workdir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(str::to_owned));

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let mut files = Vec::new();
    for entry in repo.statuses(Some(&mut opts))?.iter() {
        let bits = entry.status();
        // Ignored entries are noise for this use case.
        if bits.contains(git2::Status::IGNORED) {
            continue;
        }
        // Untracked comes through as WT_NEW; present it as workdir new.
        let index = if bits.is_wt_new() && !bits.is_index_new() {
            None
        } else {
            // INDEX_* bits occupy 0x0001–0x0010 in libgit2's layout.
            kind_of(bits & git2::Status::from_bits_truncate(0x1f), StatusKind::New)
        };
        // WT_* bits occupy 0x0080–0x1000 in libgit2's GIT_STATUS_* layout.
        const WT: git2::Status = git2::Status::from_bits_truncate(0x1f80);
        let workdir = kind_of(bits & WT, StatusKind::New);
        let path = entry
            .path()
            .map(str::to_owned)
            .unwrap_or_default();
        files.push(FileStatus {
            path,
            index,
            workdir,
        });
    }

    Ok(RepoStatus {
        root,
        branch,
        files,
    })
}

fn kind_of(bits: git2::Status, new_kind: StatusKind) -> Option<StatusKind> {
    if bits.is_empty() {
        None
    } else if bits.intersects(git2::Status::CONFLICTED) {
        Some(StatusKind::Conflicted)
    } else if bits.intersects(git2::Status::INDEX_RENAMED | git2::Status::WT_RENAMED) {
        Some(StatusKind::Renamed)
    } else if bits.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
        Some(StatusKind::Deleted)
    } else if bits.intersects(git2::Status::INDEX_TYPECHANGE | git2::Status::WT_TYPECHANGE) {
        Some(StatusKind::TypeChanged)
    } else if bits
        .intersects(git2::Status::INDEX_MODIFIED | git2::Status::WT_MODIFIED)
    {
        Some(StatusKind::Modified)
    } else {
        Some(new_kind)
    }
}

/// Unified diff of the repository containing `path`.
///
/// `staged == false`: working tree vs index (i.e. unstaged changes).
/// `staged == true`: index vs HEAD (a missing HEAD is treated as the empty
/// tree, so diffs work in freshly initialized repos).
pub fn diff(path: &Path, staged: bool) -> Result<DiffSet, GitError> {
    diff_opts(path, staged, None)
}

/// Diff restricted to one file (`None` when the file has no changes).
pub fn diff_file(path: &Path, staged: bool, file: &str) -> Result<Option<FileDiff>, GitError> {
    let set = diff_opts(path, staged, Some(file))?;
    Ok(set.files.into_iter().next())
}

fn diff_opts(path: &Path, staged: bool, file: Option<&str>) -> Result<DiffSet, GitError> {
    let repo = open(path)?;
    let mut opts = git2::DiffOptions::new();
    if let Some(file) = file {
        opts.pathspec(file);
    }
    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let index = repo.index()?;
        repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))?
    } else {
        let index = repo.index()?;
        repo.diff_index_to_workdir(Some(&index), Some(&mut opts))?
    };

    let mut files = Vec::new();
    let mut truncated = false;
    let mut bytes = 0usize;

    'outer: for delta_idx in 0..diff.deltas().len() {
        let delta = diff.get_delta(delta_idx).expect("index in range");
        let new_path = delta
            .new_file()
            .path()
            .unwrap_or_else(|| Path::new(""))
            .display()
            .to_string();
        let is_binary = delta.flags().is_binary();

        let mut file = FileDiff {
            path: new_path.clone(),
            old_path: delta
                .old_file()
                .path()
                .filter(|p| p.display().to_string() != new_path)
                .map(|p| p.display().to_string()),
            is_binary,
            hunks: Vec::new(),
        };

        if !is_binary {
            if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, delta_idx) {
                for hunk_idx in 0..patch.num_hunks() {
                    let (hunk, line_count) = patch.hunk(hunk_idx)?;
                    let mut lines = Vec::with_capacity(line_count);
                    for line_idx in 0..line_count {
                        let line = patch.line_in_hunk(hunk_idx, line_idx)?;
                        let content = String::from_utf8_lossy(line.content()).into_owned();
                        bytes += content.len();
                        lines.push(DiffLine {
                            origin: line.origin(),
                            content,
                        });
                    }
                    let header = String::from_utf8_lossy(hunk.header()).trim().to_string();
                    bytes += header.len();
                    file.hunks.push(Hunk {
                        header,
                        old_start: hunk.old_start(),
                        new_start: hunk.new_start(),
                        lines,
                    });
                    if bytes > MAX_DIFF_BYTES {
                        truncated = true;
                        break 'outer;
                    }
                }
            }
        }
        bytes += new_path.len();
        files.push(file);
        if bytes > MAX_DIFF_BYTES {
            truncated = true;
            break;
        }
    }

    Ok(DiffSet { files, truncated })
}

/// Directory (under the repo workdir) that holds all Athena-managed
/// worktrees. Keeping every agent worktree under one prefix makes the
/// sandbox invariant in [`remove_worktree`] a simple prefix check.
pub const WORKTREE_PREFIX: &str = ".athena/worktrees";

fn validate_worktree_name(name: &str) -> Result<(), GitError> {
    if name.is_empty()
        || name.len() > 64
        || name == "."
        || name == ".."
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(GitError::Git(format!(
            "invalid worktree name: {name:?} (allowed: [A-Za-z0-9._-], max 64 chars)"
        )));
    }
    Ok(())
}

fn branch_name(name: &str) -> String {
    format!("athena/{name}")
}

/// Create an agent worktree at `<root>/.athena/worktrees/<name>` on a new
/// branch `athena/<name>` based at HEAD. Returns the worktree path.
///
/// Fails for repositories with no commits (there is no HEAD to base the
/// branch on) and when the worktree or branch name is already taken.
pub fn add_worktree(repo_path: &Path, name: &str) -> Result<PathBuf, GitError> {
    validate_worktree_name(name)?;
    let repo = open(repo_path)?;
    let root = repo
        .workdir()
        .ok_or_else(|| GitError::Git("bare repositories cannot hold worktrees".into()))?;
    let commit = repo.head()?.peel_to_commit().map_err(|_| {
        GitError::Git("cannot create a worktree: repository has no commits".into())
    })?;

    if repo
        .find_branch(&branch_name(name), git2::BranchType::Local)
        .is_ok()
    {
        return Err(GitError::Git(format!(
            "branch already exists: {}",
            branch_name(name)
        )));
    }

    let dir = root.join(WORKTREE_PREFIX);
    std::fs::create_dir_all(&dir)
        .map_err(|e| GitError::Git(format!("cannot create {}: {e}", dir.display())))?;
    let path = dir.join(name);

    let branch_ref = repo
        .branch(&branch_name(name), &commit, false)?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&branch_ref));
    repo.worktree(name, &path, Some(&opts))?;
    Ok(canonicalize(path))
}

/// Remove the worktree named `name` (created via [`add_worktree`]) and its
/// on-disk directory. The backing branch `athena/<name>` is kept so agent
/// commits are never destroyed by cleanup.
pub fn remove_worktree(repo_path: &Path, name: &str) -> Result<(), GitError> {
    validate_worktree_name(name)?;
    let repo = open(repo_path)?;
    let root = repo
        .workdir()
        .ok_or_else(|| GitError::Git("bare repositories cannot hold worktrees".into()))?;
    let expected = canonicalize(root.join(WORKTREE_PREFIX).join(name));

    // Defense in depth: never prune anything outside the managed prefix.
    let prefix = canonicalize(root.join(WORKTREE_PREFIX));
    if !expected.starts_with(&prefix) {
        return Err(GitError::Git(format!(
            "refusing to remove worktree outside {WORKTREE_PREFIX}: {}",
            expected.display()
        )));
    }

    match repo.find_worktree(name) {
        Ok(wt) => {
            let mut opts = git2::WorktreePruneOptions::new();
            opts.valid(true).locked(false).working_tree(true);
            wt.prune(Some(&mut opts))?;
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    // Prune only removes the book-keeping once the tree is gone; delete any
    // remaining directory contents ourselves.
    if expected.exists() {
        std::fs::remove_dir_all(&expected)
            .map_err(|e| GitError::Git(format!("cannot remove {}: {e}", expected.display())))?;
    }
    Ok(())
}


/// What to do with a single hunk of the *unstaged* (workdir vs index) diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HunkAction {
    /// Move the hunk's changes into the index (partial `git add`).
    Stage,
    /// Reverse the hunk in the working tree (partial `git checkout --`).
    Discard,
}

fn file_diff_text(repo_path: &Path, rel_file: &str, staged: bool) -> Result<String, GitError> {
    if rel_file.is_empty()
        || rel_file.starts_with('/')
        || rel_file.split('/').any(|seg| seg == ".." || seg.is_empty())
    {
        return Err(GitError::Git(format!("invalid pathspec: {rel_file:?}")));
    }
    let repo = open(repo_path)?;
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(rel_file).force_text(false);
    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let index = repo.index()?;
        repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))?
    } else {
        let index = repo.index()?;
        repo.diff_index_to_workdir(Some(&index), Some(&mut opts))?
    };
    // Ignore whitespace churn? No — the reviewer must see byte-accurate
    // changes; the patch text is reused verbatim for hunk application.
    let mut buf = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        let content = line.content();
        // libgit2 strips the leading marker from context/add/del line content;
        // put it back. Header ('F', 'H') and "\\ No newline..." marker lines
        // ('=', '<', '>') carry their content verbatim.
        if matches!(origin, '+' | '-' | ' ') {
            buf.push(origin);
        }
        buf.push_str(&String::from_utf8_lossy(content));
        // Non-data lines always terminate; data lines without a trailing
        // newline precede a "\ No newline at end of file" marker and must
        // stay unterminated.
        if !matches!(origin, '+' | '-' | ' ') && !content.ends_with(b"\n") {
            buf.push('\n');
        }
        true
    })?;
    Ok(buf)
}

/// Split a full-file unified-diff patch into (header, hunks) where the header
/// is everything before the first "@@" line.
fn split_patch(text: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("@@") {
            hunks.push(line.to_string());
        } else if hunks.is_empty() {
            header.push_str(line);
        } else {
            hunks.last_mut().expect("hunk exists").push_str(line);
        }
    }
    (header, hunks)
}

/// Reverse a single-hunk unified-diff patch in place: swap a/…b/ path lines
/// and index ids, flip the hunk range header, and exchange +/- at
/// line-start. Returns `Err`-free text; libgit2 validates on parse.
fn reverse_patch(patch: &str) -> String {
    // Forward (old, new) path names from the diff --git line; the reversed
    // header's ---/+++ lines reference (new, old) respectively while keeping
    // git's mandatory a/ and b/ prefixes.
    let mut old_path = "";
    let mut new_path = "";
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some((a, b)) = rest.split_once(' ') {
                old_path = a.strip_prefix("a/").unwrap_or(a);
                new_path = b.strip_prefix("b/").unwrap_or(b);
            }
            break;
        }
    }
    patch
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("diff --git ") {
                // "diff --git a/x b/y" → "diff --git a/y b/x".
                let (a, b) = rest.split_once(' ').unwrap_or((rest, rest));
                let a = a.strip_prefix("a/").unwrap_or(a);
                let b = b.strip_prefix("b/").unwrap_or(b);
                format!("diff --git a/{b} b/{a}")
            } else if line.starts_with("--- ") {
                format!("--- a/{new_path}")
            } else if line.starts_with("+++ ") {
                format!("+++ b/{old_path}")
            } else if line.starts_with("index ") {
                // "index <old>..<new> <mode>" → swap oids.
                let body = line.strip_prefix("index ").unwrap_or(line);
                let mut parts = body.split_whitespace();
                let range = parts.next().unwrap_or("");
                let mode = parts.next().unwrap_or("");
                let (old, new) = range
                    .split_once("..")
                    .unwrap_or(("", ""));
                format!("index {new}..{old} {mode}").trim_end().to_string()
            } else if line.starts_with("@@") && line.contains("@@") {
                reverse_hunk_header(line)
            } else if let Some(rest) = line.strip_prefix('+') {
                format!("-{rest}")
            } else if let Some(rest) = line.strip_prefix('-') {
                format!("+{rest}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Patch text must end with a newline; `lines()`/`join` drops it and
/// libgit2 rejects the truncated last instruction.
fn with_trailing_newline(text: String) -> String {
    if text.ends_with('\n') { text } else { format!("{text}\n") }
}

/// "@@ -o1,oN +n1,nN @@" → "@@ -n1,nN +o1,oN @@" (counts may be elided).
fn reverse_hunk_header(line: &str) -> String {
    let Some(start) = line.find('-') else { return line.to_string() };
    let Some(mid) = line.find('+') else { return line.to_string() };
    let Some(end) = line[mid..].find("@@").map(|i| mid + i) else {
        return line.to_string();
    };
    let old_part = line[start..mid].trim_end();
    let new_part = line[mid..end].trim_end();
    let tail = &line[end..];
    format!("@@ -{} +{} {}", &new_part[1..], &old_part[1..], tail).to_string()
}

/// Apply exactly one hunk of `rel_file`'s unstaged diff.
///
/// `stage` applies the hunk forward to the index (partial stage);
/// `discard` applies the hunk reversed to the working tree (partial revert).
pub fn apply_hunk(
    repo_path: &Path,
    rel_file: &str,
    hunk_index: usize,
    action: HunkAction,
) -> Result<(), GitError> {
    // Bi-directional sanity: hunk ops are defined on the workdir↔index axis.
    let text = file_diff_text(repo_path, rel_file, false)?;
    let (mut header, hunks) = split_patch(&text);
    let body = hunks
        .get(hunk_index)
        .ok_or_else(|| GitError::Git(format!("no hunk #{hunk_index} in {rel_file}")))?
        .clone();
    let single = format!("{}{}", header, body);
    let single = with_trailing_newline(single);
    let applied = match action {
        HunkAction::Stage => single,
        HunkAction::Discard => with_trailing_newline(reverse_patch(&single)),
    };
    let parsed = git2::Diff::from_buffer(applied.as_bytes())
        .map_err(|e| GitError::Git(format!("hunk patch failed to parse: {e}")))?;
    let repo = open(repo_path)?;
    let location = match action {
        HunkAction::Stage => git2::ApplyLocation::Index,
        HunkAction::Discard => git2::ApplyLocation::WorkDir,
    };
    header.clear();
    repo.apply(&parsed, location, None)
        .map_err(|e| GitError::Git(format!("could not apply hunk: {e}")))
}

/// File-level counterpart of [`apply_hunk`]: stage (index gains the workdir
/// file) or discard (workdir resets to the index version, or the untracked
/// file is deleted) for a single path.
pub fn apply_file(repo_path: &Path, rel_file: &str, stage: bool) -> Result<(), GitError> {
    if rel_file.is_empty()
        || rel_file.starts_with('/')
        || rel_file.split('/').any(|seg| seg == ".." || seg.is_empty())
    {
        return Err(GitError::Git(format!("invalid pathspec: {rel_file:?}")));
    }
    let repo = open(repo_path)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| GitError::Git("bare repository".into()))?
        .to_path_buf();
    let abs = workdir.join(rel_file);
    let mut index = repo.index()?;
    let in_index = index.get_path(Path::new(rel_file), 0).is_some();
    if stage {
        if abs.exists() {
            index.add_path(Path::new(rel_file))?;
        } else {
            index.remove_path(Path::new(rel_file))?;
        }
        index.write()?;
        Ok(())
    } else if in_index {
        drop(index);
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.path(rel_file).force().remove_untracked(false);
        repo.checkout_index(None, Some(&mut checkout))?;
        Ok(())
    } else {
        // Untracked file: discard means delete.
        match std::fs::remove_file(&abs) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(GitError::Git(format!("cannot remove {}: {e}", abs.display()))),
        }
    }
}


/// Git-backed workspace snapshot: a commit of the full workdir state on a
/// dedicated ref under `refs/athena/checkpoints/`, plus (in the app layer) a
/// copy of the UI store file. User stashes, HEAD, and the real index are
/// NEVER touched by create/restore below.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    /// Commit oid (unique per snapshot).
    pub id: String,
    pub label: String,
    /// Unix seconds of the snapshot commit.
    pub created_at: i64,
}

const CHECKPOINT_REF_PREFIX: &str = "refs/athena/checkpoints/";
/// Bounded storage: older checkpoints are pruned past this count.
pub const MAX_CHECKPOINTS: usize = 20;

const CHECKPOINT_DIR: &str = "athena-checkpoints";

fn checkpoint_storage_dir(repo: &git2::Repository) -> std::path::PathBuf {
    // Linked worktrees live at <main>/.git/worktrees/<name>/ — share
    // checkpoint storage with the main repository directory.
    let gitdir = repo.path();
    let base = match gitdir
        .parent()
        .filter(|p| p.file_name().map(|n| n == "worktrees").unwrap_or(false))
    {
        Some(worktrees_dir) => worktrees_dir.parent().unwrap_or(gitdir),
        None => gitdir,
    };
    base.join(CHECKPOINT_DIR)
}

/// Snapshot the entire workdir (tracked, staged, and untracked content,
/// honoring .gitignore) as a commit, plus an optional sidecar file copy
/// (the app store). Returns the created checkpoint.
///
/// Implementation detail: we temporarily swap the repository's index for a
/// copy, `add_all` the workdir into it, write that tree, and commit it on a
/// checkpoint ref. The real `.git/index` and HEAD are untouched.
pub fn checkpoint_create(
    repo_path: &Path,
    label: &str,
    store_snapshot: Option<&Path>,
) -> Result<Checkpoint, GitError> {
    let repo = open(repo_path)?;
    let head = repo.head()?.peel_to_commit().map_err(|_| {
        GitError::Git("cannot checkpoint: repository has no commits".into())
    })?;

    let storage = checkpoint_storage_dir(&repo);
    std::fs::create_dir_all(&storage)
        .map_err(|e| GitError::Git(format!("cannot create {}: {e}", storage.display())))?;

    // Build the snapshot tree in a scratch copy of the real index.
    let index_path = repo.path().join("index");
    let scratch_path = storage.join(format!("index-scratch-{}", std::process::id()));
    let from_scratch_file = index_path.exists();
    let mut index = if from_scratch_file {
        std::fs::copy(&index_path, &scratch_path)
            .map_err(|e| GitError::Git(format!("cannot copy index: {e}")))?;
        git2::Index::open(&scratch_path)?
    } else {
        // No .git/index yet (unusual but legal): start from HEAD's tree.
        let mut index = git2::Index::new()?;
        index.read_tree(&head.tree()?)?;
        index
    };
    repo.set_index(&mut index)?;
    index.add_all(
        ["*"].iter(),
        git2::IndexAddOption::DEFAULT,
        Some(&mut |_p: &Path, _content: &[u8]| -> i32 { 0 }),
    )?;
    if from_scratch_file {
        // Only the scratch file is written — the real index is untouched.
        index.write()?;
    }
    let tree_oid = index.write_tree()?;
    drop(index);
    let _ = std::fs::remove_file(&scratch_path);

    let label_trimmed = label.trim();
    let message = if label_trimmed.is_empty() {
        "athena checkpoint".to_string()
    } else {
        format!("athena checkpoint: {label_trimmed}")
    };
    let sig = git2::Signature::now("Athena Checkpoints", "checkpoint@athena.local")?;
    let tree = repo.find_tree(tree_oid)?;
    // Refname carries a sortable time+sequence id so same-second snapshots
    // still prune in creation order (commit times are second-resolution).
    use std::sync::atomic::{AtomicU64, Ordering as AOrd};
    static _SEQ: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "{:010}-{:06}",
        sig.when().seconds(),
        _SEQ.fetch_add(1, AOrd::Relaxed)
    );
    let commit_oid = repo.commit(None, &sig, &sig, &message, &tree, &[&head])?;
    repo.reference(
        &format!("{CHECKPOINT_REF_PREFIX}{id}"),
        commit_oid,
        false,
        "athena checkpoint snapshot",
    )?;

    if let Some(store_file) = store_snapshot {
        if store_file.exists() {
            std::fs::copy(store_file, storage.join(format!("{id}.store.json")))
                .map_err(|e| GitError::Git(format!("cannot copy store snapshot: {e}")))?;
        }
    }

    let checkpoint = Checkpoint {
        id: id.clone(),
        label: label_trimmed.to_string(),
        created_at: sig.when().seconds(),
    };
    prune_checkpoints(&repo)?;
    Ok(checkpoint)
}

fn prune_checkpoints(repo: &git2::Repository) -> Result<(), GitError> {
    let mut all = checkpoint_list_repo(repo)?;
    if all.len() <= MAX_CHECKPOINTS {
        return Ok(());
    }
    all.sort_by(|a, b| b.id.cmp(&a.id));
    for old in all.into_iter().skip(MAX_CHECKPOINTS) {
        if let Ok(mut r) = repo.find_reference(&format!("{CHECKPOINT_REF_PREFIX}{}", old.id)) {
            r.delete()?;
        }
        let store_snap = checkpoint_storage_dir(repo).join(format!("{}.store.json", old.id));
        let _ = std::fs::remove_file(store_snap);
    }
    Ok(())
}

/// All checkpoints for the repo containing `repo_path`, newest first.
pub fn checkpoint_list(repo_path: &Path) -> Result<Vec<Checkpoint>, GitError> {
    let repo = open(repo_path)?;
    checkpoint_list_repo(&repo)
}

fn checkpoint_list_repo(repo: &git2::Repository) -> Result<Vec<Checkpoint>, GitError> {
    let mut out = Vec::new();
    for reference in repo
        .references()?
        .flatten()
        .filter_map(|r| r.name().map(str::to_owned))
    {
        if let Some(id) = reference.strip_prefix(CHECKPOINT_REF_PREFIX) {
            if let Ok(commit) = repo
                .find_reference(&reference)
                .and_then(|r| r.peel_to_commit())
            {
                let label = commit
                    .message()
                    .and_then(|m| m.strip_prefix("athena checkpoint: "))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                out.push(Checkpoint {
                    id: id.to_string(),
                    label,
                    created_at: commit.time().seconds(),
                });
            }
        }
    }
    // Ids are zero-padded time + sequence, so a plain reverse sort is the
    // creation order even when commits share a timestamp second.
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// Restore a checkpoint: the workdir + index are reset to the snapshot tree;
/// HEAD is untouched. Returns whether a store sidecar snapshot was restored
/// (the in-memory store refreshes on next app launch).
pub fn checkpoint_restore(
    repo_path: &Path,
    id: &str,
    store_path: &Path,
) -> Result<bool, GitError> {
    if id.len() > 32
        || !id
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-')
    {
        return Err(GitError::Git(format!("invalid checkpoint id: {id:?}")));
    }
    let repo = open(repo_path)?;
    let commit = repo
        .find_reference(&format!("{CHECKPOINT_REF_PREFIX}{id}"))
        .and_then(|r| r.peel_to_commit())
        .map_err(|_| GitError::Git(format!("unknown checkpoint: {id}")))?;
    let tree = commit.tree()?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().remove_untracked(false);
    repo.checkout_tree(tree.as_object(), Some(&mut checkout))?;
    let mut index = repo.index()?;
    index.read_tree(&tree)?;
    index.write()?;

    // Optional app-store sidecar: copy it back so the next launch picks it up.
    let snap = checkpoint_storage_dir(&repo).join(format!("{id}.store.json"));
    if snap.exists() {
        std::fs::copy(&snap, store_path)
            .map_err(|e| GitError::Git(format!("cannot restore store snapshot: {e}")))?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempRepo(PathBuf);

    impl TempRepo {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "athena-git-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            if dir.exists() {
                fs::remove_dir_all(&dir).unwrap();
            }
            fs::create_dir_all(&dir).unwrap();
            git2::Repository::init(&dir).unwrap();
            TempRepo(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn repo(&self) -> git2::Repository {
            git2::Repository::open(&self.0).unwrap()
        }

        fn commit_file(&self, rel: &str, content: &str, message: &str) {
            fs::write(self.0.join(rel), content).unwrap();
            let repo = self.repo();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new(rel)).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Athena Test", "test@athena.dev").unwrap();
            let parents: Vec<git2::Commit> = repo
                .head()
                .ok()
                .and_then(|h| h.peel_to_commit().ok())
                .into_iter()
                .collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
                .unwrap();
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discover_finds_repo_from_nested_dir() {
        let tmp = TempRepo::new();
        let nested = tmp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        let found = discover_repo(&nested).unwrap().unwrap();
        assert_eq!(found, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn discover_returns_none_outside_repo() {
        let dir = std::env::temp_dir().join(format!(
            "athena-git-notrepo-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        assert!(discover_repo(&dir).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reports_untracked_and_staged() {
        let tmp = TempRepo::new();
        tmp.commit_file("tracked.txt", "v1", "initial");

        fs::write(tmp.path().join("untracked.txt"), "hi").unwrap();
        fs::write(tmp.path().join("staged.txt"), "hi").unwrap();
        let repo = tmp.repo();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        fs::write(tmp.path().join("tracked.txt"), "v2").unwrap();

        let status = status(tmp.path()).unwrap();
        assert!(matches!(status.branch.as_deref(), Some("master") | Some("main")));

        let untracked = status
            .files
            .iter()
            .find(|f| f.path == "untracked.txt")
            .expect("untracked listed");
        assert_eq!(untracked.index, None);
        assert_eq!(untracked.workdir, Some(StatusKind::New));

        let staged = status
            .files
            .iter()
            .find(|f| f.path == "staged.txt")
            .expect("staged listed");
        assert_eq!(staged.index, Some(StatusKind::New));
        assert_eq!(staged.workdir, None);

        let modified = status
            .files
            .iter()
            .find(|f| f.path == "tracked.txt")
            .expect("modified listed");
        assert_eq!(modified.workdir, Some(StatusKind::Modified));
    }

    #[test]
    fn unstaged_diff_has_parsed_hunks() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "alpha\nbeta\ngamma\n", "initial");
        fs::write(tmp.path().join("f.txt"), "alpha\nBETA\ngamma\ndelta\n").unwrap();

        let set = diff(tmp.path(), false).unwrap();
        assert!(!set.truncated);
        assert_eq!(set.files.len(), 1);
        let file = &set.files[0];
        assert_eq!(file.path, "f.txt");
        assert!(!file.is_binary);
        let hunk = file.hunks.first().expect("one hunk");
        assert!(hunk.header.starts_with("@@"));
        let deletions = hunk.lines.iter().filter(|l| l.origin == '-').count();
        let additions = hunk.lines.iter().filter(|l| l.origin == '+').count();
        assert_eq!(deletions, 1);
        assert_eq!(additions, 2);
    }

    #[test]
    fn staged_diff_matches_staged_content_only() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "one\n", "initial");

        fs::write(tmp.path().join("f.txt"), "one\ntwo\n").unwrap();
        let repo = tmp.repo();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        // A further unstaged edit must not appear in the staged diff.
        fs::write(tmp.path().join("f.txt"), "one\ntwo\nthree\n").unwrap();

        let staged = diff(tmp.path(), true).unwrap();
        let text: String = staged.files[0]
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| l.content.clone()))
            .collect();
        assert!(text.contains("two"));
        assert!(!text.contains("three"));

        let unstaged = diff(tmp.path(), false).unwrap();
        let text: String = unstaged.files[0]
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| l.content.clone()))
            .collect();
        assert!(text.contains("three"));
    }

    #[test]
    fn staged_diff_works_without_head() {
        let tmp = TempRepo::new();
        fs::write(tmp.path().join("new.txt"), "fresh\n").unwrap();
        let repo = tmp.repo();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let set = diff(tmp.path(), true).unwrap();
        assert_eq!(set.files.len(), 1);
        assert_eq!(set.files[0].path, "new.txt");
    }

    #[test]
    fn worktree_add_create_branch_and_remove() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");

        let wt_path = add_worktree(tmp.path(), "agent-alpha").unwrap();
        assert_eq!(
            wt_path,
            tmp.path()
                .canonicalize()
                .unwrap()
                .join(".athena/worktrees/agent-alpha")
        );
        // HEAD content is present in the new worktree.
        assert_eq!(
            fs::read_to_string(wt_path.join("f.txt")).unwrap(),
            "v1\n"
        );
        // The backing branch exists and points at HEAD.
        {
            let repo = tmp.repo();
            let branch_id = repo
                .find_branch("athena/agent-alpha", git2::BranchType::Local)
                .unwrap()
                .get()
                .peel_to_commit()
                .unwrap()
                .id();
            let head_id = repo.head().unwrap().peel_to_commit().unwrap().id();
            assert_eq!(branch_id, head_id);
        }

        // Duplicate name is an error, not a clobber.
        assert!(add_worktree(tmp.path(), "agent-alpha").is_err());

        remove_worktree(tmp.path(), "agent-alpha").unwrap();
        assert!(!wt_path.join("f.txt").exists());
        // Branch survives teardown: agent commits are never destroyed.
        let repo = tmp.repo();
        assert!(
            repo.find_branch("athena/agent-alpha", git2::BranchType::Local)
                .is_ok()
        );

        // Removing a missing worktree is a no-op.
        remove_worktree(tmp.path(), "agent-alpha").unwrap();
    }

    #[test]
    fn three_agent_swarm_isolation_and_teardown() {
        // Acceptance mirror: a 3-agent swarm in a repo gets 3 worktrees on
        // 3 branches; teardown removes the directories, branches remain.
        let tmp = TempRepo::new();
        tmp.commit_file("main.rs", "fn main() {}\n", "initial");

        let roles = ["coordinator", "builder", "scout"];
        let mut paths = Vec::new();
        for role in roles {
            let name = format!("{role}-abc123");
            let path = add_worktree(tmp.path(), &name).unwrap();
            assert!(path.join("main.rs").exists(), "{} missing HEAD content", role);
            paths.push(path);
        }
        // All three live side by side under the managed prefix.
        assert_eq!(paths.len(), 3);
        let repo = tmp.repo();
        for role in roles {
            let branch = repo
                .find_branch(&format!("athena/{role}-abc123"), git2::BranchType::Local)
                .unwrap_or_else(|e| panic!("missing branch for {role}: {e}"));
            drop(branch);
        }
        drop(repo);

        // Independent edits do not leak between agents.
        fs::write(paths[1].join("main.rs"), "fn main() { builder }\n").unwrap();
        assert_eq!(
            fs::read_to_string(paths[0].join("main.rs")).unwrap(),
            "fn main() {}\n"
        );

        for role in roles {
            remove_worktree(tmp.path(), &format!("{role}-abc123")).unwrap();
        }
        for path in &paths {
            assert!(!path.exists(), "{} not torn down", path.display());
        }
        let repo = tmp.repo();
        for role in roles {
            assert!(
                repo.find_branch(&format!("athena/{role}-abc123"), git2::BranchType::Local)
                    .is_ok(),
                "branch for {role} must survive teardown"
            );
        }
    }

    #[test]
    fn worktree_add_rejects_bad_names_and_headless_repo() {
        let tmp = TempRepo::new();
        // No commits yet: cannot base a worktree branch.
        assert!(matches!(
            add_worktree(tmp.path(), "ok"),
            Err(GitError::Git(_))
        ));
        for bad in ["", "../escape", "a/b", "a b", ".."] {
            assert!(add_worktree(tmp.path(), bad).is_err(), "name {bad:?}");
            assert!(remove_worktree(tmp.path(), bad).is_err(), "name {bad:?}");
        }
    }


    #[test]
    fn checkpoint_create_restore_roundtrip() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");
        // Untracked file participating in the snapshot.
        fs::write(tmp.path().join("scratch.txt"), "draft\n").unwrap();

        let cp = checkpoint_create(tmp.path(), "before agent", None).unwrap();
        assert_eq!(cp.label, "before agent");

        // Break things.
        fs::write(tmp.path().join("f.txt"), "broken\n").unwrap();
        fs::remove_file(tmp.path().join("scratch.txt")).unwrap();

        // Restore: workdir back to snapshot.
        let store_snap = tmp.path().join("store.json");
        fs::write(&store_snap, "{}").unwrap();
        let restored = checkpoint_restore(tmp.path(), &cp.id, &store_snap).unwrap();
        assert!(!restored); // no sidecar written on create
        assert_eq!(fs::read_to_string(tmp.path().join("f.txt")).unwrap(), "v1\n");
        assert_eq!(
            fs::read_to_string(tmp.path().join("scratch.txt")).unwrap(),
            "draft\n"
        );
        // HEAD untouched; index matches the snapshot tree.
        let repo = tmp.repo();
        let head_content = String::from_utf8_lossy(
            &repo
                .head()
                .unwrap()
                .peel_to_tree()
                .unwrap()
                .get_name("f.txt")
                .map(|e| {
                    repo.find_blob(e.id()).unwrap().content().to_vec()
                })
                .unwrap(),
        )
        .to_string();
        assert_eq!(head_content, "v1\n");
        assert!(diff(tmp.path(), false).unwrap().files.is_empty());
        drop(repo);
    }

    #[test]
    fn checkpoint_prunes_beyond_twenty() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");
        for i in 0..22 {
            fs::write(tmp.path().join("f.txt"), format!("v{i}\n")).unwrap();
            checkpoint_create(tmp.path(), &format!("snap{i}"), None).unwrap();
        }
        let all = checkpoint_list(tmp.path()).unwrap();
        assert!(all.len() <= MAX_CHECKPOINTS, "got {}", all.len());
        // Newest kept.
        assert_eq!(all[0].label, "snap21");
    }

    #[test]
    fn checkpoint_restore_rejects_bad_ids() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");
        checkpoint_create(tmp.path(), "x", None).unwrap();
        for bad in ["", "../..", "not-hex", &"1".repeat(40)] {
            assert!(
                checkpoint_restore(tmp.path(), bad, Path::new("/tmp/x.json")).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn store_sidecar_roundtrip() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");
        let store = tmp.path().join("app-store.json");
        fs::write(&store, "{\"k\":1}").unwrap();
        let cp = checkpoint_create(tmp.path(), "with-store", Some(&store)).unwrap();
        fs::write(&store, "{\"k\":2}").unwrap();
        let restored = checkpoint_restore(tmp.path(), &cp.id, &store).unwrap();
        assert!(restored);
        assert_eq!(fs::read_to_string(&store).unwrap(), "{\"k\":1}");
    }

    #[test]
    fn diff_file_filters_single_path() {
        let tmp = TempRepo::new();
        tmp.commit_file("a.txt", "1\n", "initial");
        tmp.commit_file("b.txt", "2\n", "second");
        fs::write(tmp.path().join("a.txt"), "1x\n").unwrap();
        fs::write(tmp.path().join("b.txt"), "2x\n").unwrap();

        let one = diff_file(tmp.path(), false, "a.txt").unwrap().unwrap();
        assert_eq!(one.path, "a.txt");
        assert_eq!(one.hunks.len(), 1);

        // Clean file returns None, missing file too.
        tmp.commit_file("a.txt", "1x\n", "fixup");
        assert!(diff_file(tmp.path(), false, "a.txt").unwrap().is_none());
        assert!(diff_file(tmp.path(), false, "nope.txt").unwrap().is_none());
    }

    #[test]
    fn apply_hunk_stages_single_hunk() {
        let tmp = TempRepo::new();
        // Two well-separated regions so two hunks.
        let base: String = (1..=20).map(|i| format!("line{i}\n")).collect();
        tmp.commit_file("f.txt", &base, "initial");
        let changed: String = (1..=20)
            .map(|i| if i == 1 || i == 20 { format!("CHANGED{i}\n") } else { format!("line{i}\n") })
            .collect();
        fs::write(tmp.path().join("f.txt"), &changed).unwrap();

        // Sanity: two hunks.
        let set = diff(tmp.path(), false).unwrap();
        assert_eq!(set.files.len(), 1);
        assert_eq!(set.files[0].hunks.len(), 2);

        apply_hunk(tmp.path(), "f.txt", 0, HunkAction::Stage).unwrap();

        // Staged diff contains CHANGED1 but not CHANGED20; unstaged keeps 20.
        let staged = diff(tmp.path(), true).unwrap();
        let staged_text: String = staged.files[0]
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| l.content.clone()))
            .collect();
        assert!(staged_text.contains("CHANGED1"), "{staged_text}");
        assert!(!staged_text.contains("CHANGED20"), "{staged_text}");

        let unstaged = diff(tmp.path(), false).unwrap();
        let unstaged_text: String = unstaged.files[0]
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| l.content.clone()))
            .collect();
        assert!(!unstaged_text.contains("CHANGED1"), "{unstaged_text}");
        assert!(unstaged_text.contains("CHANGED20"), "{unstaged_text}");
    }

    #[test]
    fn apply_hunk_discard_reverts_single_hunk() {
        let tmp = TempRepo::new();
        let base: String = (1..=20).map(|i| format!("line{i}\n")).collect();
        tmp.commit_file("f.txt", &base, "initial");
        let changed: String = (1..=20)
            .map(|i| if i == 2 || i == 19 { "X\n".to_string() } else { format!("line{i}\n") })
            .collect();
        fs::write(tmp.path().join("f.txt"), &changed).unwrap();

        apply_hunk(tmp.path(), "f.txt", 1, HunkAction::Discard).unwrap();

        let on_disk = fs::read_to_string(tmp.path().join("f.txt")).unwrap();
        assert!(on_disk.contains("X\n"), "hunk 0 must survive discard of hunk 1: {on_disk:?}");
        assert!(on_disk.contains("line19"), "{on_disk:?}");

        // Only one hunk remains unstaged.
        let set = diff(tmp.path(), false).unwrap();
        assert_eq!(set.files[0].hunks.len(), 1);
    }

    #[test]
    fn apply_hunk_out_of_range_errors_and_is_atomic() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "a\n", "initial");
        fs::write(tmp.path().join("f.txt"), "a\nb\n").unwrap();
        assert!(matches!(
            apply_hunk(tmp.path(), "f.txt", 7, HunkAction::Stage),
            Err(GitError::Git(_))
        ));
        // Nothing moved.
        let staged = diff(tmp.path(), true).unwrap();
        assert!(staged.files.is_empty());
    }

    #[test]
    fn apply_hunk_rejects_traversal() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "a\n", "initial");
        for bad in ["../x", "/etc/passwd", "a//b", ""] {
            for action in [HunkAction::Stage, HunkAction::Discard] {
                assert!(apply_hunk(tmp.path(), bad, 0, action).is_err(), "{bad:?}");
            }
        }
    }

    #[test]
    fn apply_file_stage_and_discard() {
        let tmp = TempRepo::new();
        tmp.commit_file("f.txt", "v1\n", "initial");

        // Modified file: stage then discard restores index version.
        fs::write(tmp.path().join("f.txt"), "v2\n").unwrap();
        apply_file(tmp.path(), "f.txt", true).unwrap();
        let staged = diff(tmp.path(), true).unwrap();
        assert_eq!(staged.files.len(), 1);
        fs::write(tmp.path().join("f.txt"), "v3\n").unwrap();
        apply_file(tmp.path(), "f.txt", false).unwrap();
        assert_eq!(fs::read_to_string(tmp.path().join("f.txt")).unwrap(), "v2\n");

        // Untracked file: stage then discard deletes.
        fs::write(tmp.path().join("new.txt"), "fresh\n").unwrap();
        apply_file(tmp.path(), "new.txt", true).unwrap();
        let staged = diff(tmp.path(), true).unwrap();
        assert!(staged.files.iter().any(|f| f.path == "new.txt"));
        fs::write(tmp.path().join("other.txt"), "untracked\n").unwrap();
        apply_file(tmp.path(), "other.txt", false).unwrap();
        assert!(!tmp.path().join("other.txt").exists());

        // Traversal rejected.
        assert!(apply_file(tmp.path(), "../evil.txt", true).is_err());
    }

    #[test]
    fn diff_outside_repo_errors() {
        let dir = std::env::temp_dir().join(format!(
            "athena-git-diff-notrepo-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        assert!(matches!(
            diff(&dir, false),
            Err(GitError::NotARepo(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}



