//! Path validation utilities for safe file system operations.
//!
//! Provides [`PathValidator`] which enforces that all file system access
//! stays within a designated sandbox (workspace) root.  The validator
//! canonicalizes both the workspace root and the target path, then checks
//! that the latter is a true descendant of the former.  It also mitigates
//! TOCTOU and symlink-escapes by resolving the full path into its canonical
//! form before validating.

use std::path::{Path, PathBuf};

/// Errors produced by path validation.
#[derive(Debug, thiserror::Error)]
pub enum PathValidationError {
    #[error("path traversal denied: {0}")]
    PathTraversal(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Validates that a target path stays within a sandbox root.
///
/// All operations are performed on canonicalized paths so symlink tricks,
/// `..` sequences, and other traversal attempts are neutralised.
#[derive(Debug, Clone)]
pub struct PathValidator {
    root: PathBuf,
    /// Opt-in extra sandbox roots granted on top of the primary `root`.
    /// Each entry is canonicalized before being stored. Empty by default —
    /// the sandbox stays single-root unless the caller explicitly widens it.
    extra_roots: Vec<PathBuf>,
}

impl PathValidator {
    /// Create a new validator rooted at `root`.
    ///
    /// The `root` is canonicalized internally; if it does not exist the
    /// constructor returns an error.
    pub fn new(root: &Path) -> Result<Self, PathValidationError> {
        let root = root.canonicalize().map_err(|e| {
            PathValidationError::InvalidPath(format!("cannot canonicalize root: {}", e))
        })?;
        Ok(Self {
            root,
            extra_roots: Vec::new(),
        })
    }
    /// Grant additional canonical sandbox roots. Paths under any of these
    /// are accepted alongside the primary root. Roots that do not exist
    /// fail construction so a typo cannot silently widen the sandbox.
    pub fn with_extra_roots<I, P>(mut self, roots: I) -> Result<Self, PathValidationError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut extra = Vec::new();
        for root in roots {
            let root = root.as_ref().canonicalize().map_err(|e| {
                PathValidationError::InvalidPath(format!(
                    "cannot canonicalize extra root {:?}: {}",
                    root.as_ref(),
                    e
                ))
            })?;
            extra.push(root);
        }
        self.extra_roots = extra;
        Ok(self)
    }

    /// Validate that `path` is within the sandbox root.
    ///
    /// Returns the **canonical** `PathBuf` or an error if the path escapes
    /// the sandbox or does not exist.
    pub fn validate(&self, path: &Path) -> Result<PathBuf, PathValidationError> {
        let canonical = path.canonicalize().map_err(|e| {
            PathValidationError::PathTraversal(format!("cannot canonicalize {:?}: {}", path, e))
        })?;
        self.validate_canonical(&canonical)
    }

    /// Validate that `path` is within the sandbox root, allowing the path
    /// to not yet exist (e.g. for writes).  Parent directories must exist so
    /// that the canonicalisation can be verified.
    pub fn validate_write(&self, path: &Path) -> Result<PathBuf, PathValidationError> {
        // For paths that do not exist, canonicalize the parent and join the
        // final component afterwards.
        let canonical = if path.exists() {
            path.canonicalize().map_err(|e| {
                PathValidationError::PathTraversal(format!("cannot canonicalize {:?}: {}", path, e))
            })?
        } else {
            let parent = path.parent().ok_or_else(|| {
                PathValidationError::PathTraversal(format!("path {:?} has no parent", path))
            })?;
            let canonical_parent = parent.canonicalize().map_err(|e| {
                PathValidationError::PathTraversal(format!(
                    "cannot canonicalize parent of {:?}: {}",
                    path, e
                ))
            })?;
            if let Some(name) = path.file_name() {
                canonical_parent.join(name)
            } else {
                return Err(PathValidationError::PathTraversal(format!(
                    "path {:?} has no file name",
                    path
                )));
            }
        };
        self.validate_canonical(&canonical)
    }

    /// Validate that the canonicalized `path` starts with the canonicalized
    /// root.
    fn validate_canonical(&self, canonical: &Path) -> Result<PathBuf, PathValidationError> {
        let allowed = canonical.starts_with(&self.root)
            || self.extra_roots.iter().any(|r| canonical.starts_with(r));
        if !allowed {
            return Err(PathValidationError::PathTraversal(format!(
                "path {:?} is outside sandbox root {:?}",
                canonical, self.root
            )));
        }
        Ok(canonical.to_path_buf())
    }

    /// Return the canonical sandbox root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_valid_path_within_root() {
        let temp = std::env::temp_dir();
        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let sub = temp.join("athena_test_valid");
        fs::create_dir_all(&sub).unwrap();
        let result = validator.validate(&sub);
        assert!(result.is_ok());
        fs::remove_dir_all(&sub).ok();
    }

    #[test]
    fn test_path_traversal_with_dotdot() {
        let temp = std::env::temp_dir();
        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let evil = temp
            .join("foo")
            .join("..")
            .join("..")
            .join("etc")
            .join("passwd");
        let result = validator.validate(&evil);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_traversal_absolute_escape() {
        let temp = std::env::temp_dir();
        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let evil = PathBuf::from("/etc/passwd");
        let result = validator.validate(&evil);
        assert!(result.is_err());
    }

    #[test]
    fn test_symlink_escape() {
        let temp = std::env::temp_dir().join("athena_test_symlink");
        fs::create_dir_all(&temp).unwrap();
        let link = temp.join("evil_link");
        // Create a symlink pointing outside the temp dir
        let target = PathBuf::from("/etc/passwd");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).ok();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link).ok();

        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let result = validator.validate(&link);
        assert!(result.is_err());
        fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn test_write_path_nonexistent() {
        let temp = std::env::temp_dir();
        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let new_file = temp.join("athena_test_new_file.txt");
        let result = validator.validate_write(&new_file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_path_traversal_nonexistent() {
        let temp = std::env::temp_dir();
        let validator = PathValidator::new(&temp).expect("temp dir should exist");
        let evil = temp.join("..").join("..").join("etc").join("passwd");
        let result = validator.validate_write(&evil);
        assert!(result.is_err());
    }

    #[test]
    fn test_extra_roots_accept_paths_under_additional_root() {
        let base = std::env::temp_dir();
        let primary = base.join("athena_test_primary");
        let extra = base.join("athena_test_extra");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&extra).unwrap();
        let validator = PathValidator::new(&primary)
            .unwrap()
            .with_extra_roots([&extra])
            .expect("extra root should exist");
        assert!(validator.validate(&extra).is_ok());
        assert!(validator.validate(&primary).is_ok());
        // A sibling that is neither root nor extra stays blocked.
        let other = base.join("athena_test_other");
        fs::create_dir_all(&other).unwrap();
        assert!(validator.validate(&other).is_err());
        for dir in [&primary, &extra, &other] {
            fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn test_extra_roots_rejects_nonexistent_root() {
        let base = std::env::temp_dir();
        let primary = base.join("athena_test_primary2");
        fs::create_dir_all(&primary).unwrap();
        let missing = base.join("athena_test_missing_root");
        let result = PathValidator::new(&primary)
            .unwrap()
            .with_extra_roots([&missing]);
        assert!(result.is_err(), "nonexistent extra root must be rejected");
        fs::remove_dir_all(&primary).ok();
    }
}
