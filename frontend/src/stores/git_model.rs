//! Types and bounds for the git store, mirroring the `athena-git` crate's
//! serde output (field names must stay in sync with it).

/// Maximum repository snapshots kept in the store; least-recently-touched
/// entries are evicted past this so a long session can't grow the map
/// without bound.
pub const MAX_TRACKED_REPOS: usize = 16;

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
