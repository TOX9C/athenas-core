use std::fs;
use std::path::{Path, PathBuf};

pub mod path_validator;
use path_validator::PathValidator;

/// Represents a node in the file tree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Option<Vec<FileNode>>,
    /// `true` when children are omitted because the depth limit was reached.
    pub truncated: bool,
}

const SKIP_ENTRIES: &[&str] = &["node_modules", ".git", ".next", "dist", "build", ".ade", ".DS_Store"];
const MAX_DEPTH: usize = 6;
/// Maximum size [`read_file_content`] will load into memory.
const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;

/// Errors that can occur during file system operations.
#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(String),
    #[error("path traversal denied: {0}")]
    PathTraversal(String),
    /// The target path does not exist. Kept distinct from `PathTraversal` so
    /// a missing file is never reported as a security violation.
    #[error("not found: {0}")]
    NotFound(String),
    /// The file exceeds the maximum readable size; the report carries the
    /// actual size and the cap so callers can surface a truncation hint.
    #[error("file too large: {0} bytes (limit {1} bytes)")]
    FileTooLarge(u64, u64),
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        FsError::Io(e.to_string())
    }
}

impl From<path_validator::PathValidationError> for FsError {
    fn from(e: path_validator::PathValidationError) -> Self {
        use path_validator::PathValidationError::*;
        match e {
            PathTraversal(msg) => FsError::PathTraversal(msg),
            NotFound(msg) => FsError::NotFound(msg),
            InvalidPath(msg) => FsError::PathTraversal(msg),
            Io(err) => FsError::Io(err.to_string()),
        }
    }
}

/// Get the home directory as a PathBuf.
fn get_home() -> Result<PathBuf, FsError> {
    dirs::home_dir()
        .ok_or_else(|| FsError::PathTraversal("cannot determine home directory".to_string()))
}

/// Returns a `PathValidator` rooted at the home directory, with the
/// sensitive-subtree blocklist (`~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.kube`)
/// enforced. Prefer a workspace-rooted validator
/// ([`PathValidator::new_workspace`]) when the caller knows the workspace.
fn home_validator() -> Result<PathValidator, FsError> {
    let home = get_home()?;
    PathValidator::new_home(&home)
        .map_err(|e| FsError::PathTraversal(format!("failed to create home validator: {}", e)))
}

/// Recursively reads the directory tree starting at `dir`.
///
/// - `depth` controls recursion; the default is 0 and the maximum is [`MAX_DEPTH`].
/// - Entries in `SKIP_ENTRIES` and any dotfiles are ignored.
/// - Symlinks are skipped to avoid cycles.
/// - Results are sorted: directories first, then files, each sub-sorted by name.
pub fn read_tree(dir: &Path, depth: usize) -> Result<Vec<FileNode>, FsError> {
    let validator = home_validator()?;
    let canonical = validator.validate(dir)?;
    read_tree_inner(&canonical, depth)
}

/// Inner recursive walker. The root has already been validated+canonicalized
/// once by [`read_tree`]; children come from `fs::read_dir` on an
/// already-canonical directory and symlinks are skipped, so re-running
/// validate (one `canonicalize` syscall per recursion level) buys nothing.
fn read_tree_inner(canonical_dir: &Path, depth: usize) -> Result<Vec<FileNode>, FsError> {
    if depth >= MAX_DEPTH {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(canonical_dir)?
        .filter_map(|entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("skipping directory entry: {}", e);
                    return None;
                }
            };

            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') || SKIP_ENTRIES.contains(&name_str.as_ref()) {
                return None;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    log::warn!("skipping entry, cannot get file type: {}", e);
                    return None;
                }
            };

            if file_type.is_symlink() {
                return None; // skip symlinks to avoid cycles
            }

            let is_dir = file_type.is_dir();

            Some((name_str.into_owned(), entry.path(), is_dir))
        })
        .collect::<Vec<_>>();

    // Sort directories before files, then by name
    entries.sort_by(|a, b| match (a.2, b.2) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });

    let mut nodes = Vec::with_capacity(entries.len());
    for (name, path, is_directory) in entries {
        if is_directory {
            let (children, truncated) = if depth + 1 >= MAX_DEPTH {
                (Vec::new(), true)
            } else {
                // An unreadable subdirectory (permissions, races with other
                // processes) must not abort the entire tree listing. Emit it
                // as an empty, truncated node and continue.
                match read_tree_inner(&path, depth + 1) {
                    Ok(children) => (children, false),
                    Err(e) => {
                        log::warn!("skipping unreadable directory {:?}: {}", path, e);
                        (Vec::new(), true)
                    }
                }
            };
            nodes.push(FileNode {
                name,
                path: path_to_string(&path),
                is_directory: true,
                children: Some(children),
                truncated,
            });
        } else {
            nodes.push(FileNode {
                name,
                path: path_to_string(&path),
                is_directory: false,
                children: None,
                truncated: false,
            });
        }
    }

    Ok(nodes)
}

/// Reads the full text content of a file.
///
/// Files larger than [`MAX_READ_BYTES`] are rejected with
/// [`FsError::FileTooLarge`] instead of being read, so one call cannot
/// allocate unbounded memory.
pub fn read_file_content(path: &Path) -> Result<String, FsError> {
    let validator = home_validator()?;
    let canonical = validator.validate(path)?;

    let metadata = fs::metadata(&canonical)?;
    if metadata.len() > MAX_READ_BYTES {
        return Err(FsError::FileTooLarge(metadata.len(), MAX_READ_BYTES));
    }
    fs::read_to_string(&canonical).map_err(FsError::from)
}

/// Writes `content` to a file atomically by writing to a temp file then renaming.
pub fn write_file_content(path: &Path, content: &str) -> Result<(), FsError> {
    let validator = home_validator()?;
    // The canonical path returned here is what the later rename targets, so
    // a swapped symlink in the destination cannot redirect the write after
    // validation.
    let canonical = validator.validate_write(path)?;

    // Build a unique temp-file path in the SAME directory (atomic rename
    // requires same-filesystem source+dest). The previous
    // `path.with_extension("athena_tmp")` collided across sibling files that
    // shared a stem and differed only in extension (`foo.json` and `foo.toml`
    // both became `foo.athena_tmp`), and would clobber a real file literally
    // named `*.athena_tmp`. Use the full file name + PID + timestamp suffix
    // so concurrent writes never race on the same temp file.
    let temp_path = unique_temp_path(&canonical);

    // Open the temp file with O_EXCL (create_new): if a file/symlink already
    // exists at temp_path, this fails. That closes the TOCTOU window where an
    // attacker could plant a symlink named after our temp file between
    // validate_write and the write, redirecting content outside the sandbox.
    // Writing through a pre-existing handle (rather than fs::write which
    // truncates whatever is there) is what makes this safe.
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
    }

    // Atomic replace. On Unix, rename() over an existing path does NOT follow
    // a symlink at the destination (it replaces the directory entry itself),
    // so a symlink swapped in at `path` after validation is itself replaced
    // rather than written-through. On Windows, plain `fs::rename` fails when
    // the destination exists, so use MoveFileExW with REPLACE_EXISTING.
    if let Err(e) = rename_replace(&temp_path, &canonical) {
        // Never leak the temp file on a failed rename.
        let _ = fs::remove_file(&temp_path);
        return Err(e.into());
    }
    Ok(())
}

/// Replace `to` with `from` atomically, overwriting an existing destination.
#[cfg(unix)]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// Windows: `std::fs::rename` fails with `ERROR_ALREADY_EXISTS` when the
/// destination exists; `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` is the
/// correct atomic-replace call.
#[cfg(windows)]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }

    let ok = unsafe {
        MoveFileExW(
            wide(from).as_ptr(),
            wide(to).as_ptr(),
            MOVEFILE_REPLACE_EXISTING,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Fallback for other platforms: best-effort `std::fs::rename`.
#[cfg(not(any(unix, windows)))]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// Construct a same-directory temp path that won't collide with other writes
/// or with real files. Format: `<dir>/<filename>.athena_tmp.<pid>.<nanos>`.
fn unique_temp_path(path: &Path) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let temp_name = format!("{}.athena_tmp.{}.{}", file_name, pid, nanos);
    path.with_file_name(temp_name)
}

/// Returns the names of all immediate sub-directories inside `dir`.
pub fn get_directories(dir: &Path) -> Result<Vec<String>, FsError> {
    let validator = home_validator()?;
    let canonical = validator.validate(dir)?;

    let mut dirs = Vec::new();

    for entry in fs::read_dir(&canonical)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            dirs.push(name);
        }
    }

    Ok(dirs)
}

/// Helper to convert a `Path` to a `String`, lossily.
fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a temp directory inside the user's home so that
    /// `ensure_within_home` does not reject it.
    fn home_temp(sub: &str) -> PathBuf {
        let home = dirs::home_dir().expect("home directory must be available for tests");
        let dir = home.join(".athena_test_tmp").join(sub);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_read_file_content() {
        let temp_dir = home_temp("read_file");
        let test_file = temp_dir.join("athena_fs_test_read.txt");
        fs::write(&test_file, "hello world").unwrap();

        let content = read_file_content(&test_file).unwrap();
        assert_eq!(content, "hello world");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_write_file_content() {
        let temp_dir = home_temp("write_file");
        let test_file = temp_dir.join("athena_fs_test_write.txt");

        write_file_content(&test_file, "test content").unwrap();
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "test content");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_read_file_content_missing_is_not_found_not_traversal() {
        let temp_dir = home_temp("read_missing");
        let missing = temp_dir.join("definitely_missing_9c41.txt");
        match read_file_content(&missing) {
            Err(FsError::NotFound(_)) => {}
            other => panic!("expected FsError::NotFound, got {:?}", other),
        }
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_get_directories() {
        let temp_dir = home_temp("get_dirs");
        fs::create_dir_all(temp_dir.join("dir_a")).unwrap();
        fs::create_dir_all(temp_dir.join("dir_b")).unwrap();
        fs::write(temp_dir.join("file.txt"), "x").unwrap();

        let dirs = get_directories(&temp_dir).unwrap();
        assert!(dirs.contains(&"dir_a".to_string()));
        assert!(dirs.contains(&"dir_b".to_string()));
        assert!(!dirs.contains(&"file.txt".to_string()));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_read_tree_skips_dotfiles_and_skip_entries() {
        let temp_dir = home_temp("tree_skip");
        fs::create_dir_all(temp_dir.join(".hidden")).unwrap();
        fs::create_dir_all(temp_dir.join("node_modules")).unwrap();
        fs::write(temp_dir.join("visible.txt"), "x").unwrap();
        fs::write(temp_dir.join(".gitignore"), "x").unwrap();

        let tree = read_tree(&temp_dir, 0).unwrap();
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"visible.txt"));
        assert!(!names.contains(&".hidden"));
        assert!(!names.contains(&"node_modules"));
        assert!(!names.contains(&".gitignore"));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_read_tree_sorts_directories_first() {
        let temp_dir = home_temp("tree_sort");
        fs::create_dir_all(temp_dir.join("zzz_dir")).unwrap();
        fs::write(temp_dir.join("aaa_file.txt"), "x").unwrap();

        let tree = read_tree(&temp_dir, 0).unwrap();
        assert_eq!(tree.len(), 2);
        assert!(tree[0].is_directory);
        assert_eq!(tree[0].name, "zzz_dir");
        assert!(!tree[1].is_directory);
        assert_eq!(tree[1].name, "aaa_file.txt");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_read_tree_respects_max_depth() {
        let temp_dir = home_temp("tree_depth");
        let deep = temp_dir
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("f")
            .join("g");
        fs::create_dir_all(&deep).unwrap();

        let tree = read_tree(&temp_dir, 0).unwrap();
        assert!(!tree.is_empty());

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
