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
    let repo = open(path)?;
    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let index = repo.index()?;
        repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), None)?
    } else {
        let index = repo.index()?;
        repo.diff_index_to_workdir(Some(&index), None)?
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


