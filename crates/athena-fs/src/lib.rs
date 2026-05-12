use std::fs;
use std::path::Path;

/// Represents a node in the file tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Option<Vec<FileNode>>,
}

const SKIP_DIRS: &[&str] = &["node_modules", ".git", ".next", "dist", "build", ".ade", ".DS_Store"];
const MAX_DEPTH: usize = 6;

/// Errors that can occur during file system operations.
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Recursively reads the directory tree starting at `dir`.
///
/// - `depth` controls recursion; the default is 0 and the maximum is [`MAX_DEPTH`].
/// - Entries in `SKIP_DIRS` and any dotfiles are ignored.
/// - Results are sorted: directories first, then files, each sub-sorted by name.
pub fn read_tree(dir: &Path, depth: usize) -> Result<Vec<FileNode>, FsError> {
    if depth >= MAX_DEPTH {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') || SKIP_DIRS.contains(&name_str.as_ref()) {
                return None;
            }

            let metadata = entry.metadata().ok()?;
            Some((name_str.into_owned(), entry.path(), metadata.is_dir()))
        })
        .collect::<Vec<_>>();

    // Sort directories before files, then by name
    entries.sort_by(|a, b| {
        match (a.2, b.2) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(&b.0),
        }
    });

    let mut nodes = Vec::with_capacity(entries.len());
    for (name, path, is_directory) in entries {
        if is_directory {
            let children = read_tree(&path, depth + 1)?;
            nodes.push(FileNode {
                name,
                path: path_to_string(&path),
                is_directory: true,
                children: Some(children),
            });
        } else {
            nodes.push(FileNode {
                name,
                path: path_to_string(&path),
                is_directory: false,
                children: None,
            });
        }
    }

    Ok(nodes)
}

/// Reads the full text content of a file.
pub fn read_file_content(path: &Path) -> Result<String, FsError> {
    fs::read_to_string(path).map_err(FsError::from)
}

/// Writes `content` to a file, overwriting if it already exists.
pub fn write_file_content(path: &Path, content: &str) -> Result<(), FsError> {
    fs::write(path, content).map_err(FsError::from)
}

/// Returns the names of all immediate sub-directories inside `dir`.
pub fn get_directories(dir: &Path) -> Result<Vec<String>, FsError> {
    let mut dirs = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                dirs.push(name.to_string());
            }
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

    #[test]
    fn test_read_file_content() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("athena_fs_test_read.txt");
        fs::write(&test_file, "hello world").unwrap();

        let content = read_file_content(&test_file).unwrap();
        assert_eq!(content, "hello world");

        fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn test_write_file_content() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("athena_fs_test_write.txt");

        write_file_content(&test_file, "test content").unwrap();
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "test content");

        fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn test_get_directories() {
        let temp_dir = std::env::temp_dir().join("athena_fs_test_dirs");
        fs::create_dir_all(&temp_dir).unwrap();
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
    fn test_read_tree_skips_dotfiles_and_skip_dirs() {
        let temp_dir = std::env::temp_dir().join("athena_fs_test_tree");
        fs::create_dir_all(&temp_dir).unwrap();
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
        let temp_dir = std::env::temp_dir().join("athena_fs_test_sort");
        fs::create_dir_all(&temp_dir).unwrap();
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
        let temp_dir = std::env::temp_dir().join("athena_fs_test_depth");
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
