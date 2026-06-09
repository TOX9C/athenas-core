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

const SKIP_ENTRIES: &[&str] = &[
    "node_modules",
    ".git",
    ".next",
    "dist",
    "build",
    ".ade",
    ".DS_Store",
];
const MAX_DEPTH: usize = 6;

/// Errors that can occur during file system operations.
#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(String),
    #[error("path traversal denied: {0}")]
    PathTraversal(String),
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        FsError::Io(e.to_string())
    }
}

impl From<path_validator::PathValidationError> for FsError {
    fn from(e: path_validator::PathValidationError) -> Self {
        FsError::PathTraversal(e.to_string())
    }
}

/// Get the home directory as a PathBuf.
fn get_home() -> Result<PathBuf, FsError> {
    dirs::home_dir()
        .ok_or_else(|| FsError::PathTraversal("cannot determine home directory".to_string()))
}

/// Returns a `PathValidator` rooted at the home directory.
fn home_validator() -> Result<PathValidator, FsError> {
    let home = get_home()?;
    PathValidator::new(&home)
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
    let _canonical = validator.validate(dir)?;

    if depth >= MAX_DEPTH {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(dir)?
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
                (read_tree(&path, depth + 1)?, false)
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
pub fn read_file_content(path: &Path) -> Result<String, FsError> {
    let validator = home_validator()?;
    let _canonical = validator.validate(path)?;
    fs::read_to_string(path).map_err(FsError::from)
}

/// Writes `content` to a file atomically by writing to a temp file then renaming.
pub fn write_file_content(path: &Path, content: &str) -> Result<(), FsError> {
    let validator = home_validator()?;
    let _ = validator.validate_write(path)?;
    let temp_path = path.with_extension("athena_tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Returns the names of all immediate sub-directories inside `dir`.
pub fn get_directories(dir: &Path) -> Result<Vec<String>, FsError> {
    let validator = home_validator()?;
    let _canonical = validator.validate(dir)?;

    let mut dirs = Vec::new();

    for entry in fs::read_dir(dir)? {
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
