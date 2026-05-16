use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Cursor position within an editor file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

/// An open file in the editor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditorFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub is_dirty: bool,
    pub cursor_position: CursorPosition,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global editor state.
#[derive(Clone, PartialEq, Default)]
pub struct EditorState {
    pub open_files: Vec<EditorFile>,
    pub active_file_path: Option<String>,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            open_files: Vec::new(),
            active_file_path: None,
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    /// Open a file, or switch to it if already open.
    pub fn open_file(&mut self, file: EditorFile) {
        let path = file.path.clone();
        if self.open_files.iter().any(|f| f.path == path) {
            self.active_file_path = Some(path);
        } else {
            self.open_files.push(file);
            self.active_file_path = Some(path);
        }
    }

    /// Close a file by path. If the closed file was active, switch to the
    /// last remaining file (or None).
    pub fn close_file(&mut self, path: &str) {
        self.open_files.retain(|f| f.path != path);
        if self.active_file_path.as_deref() == Some(path) {
            self.active_file_path = self.open_files.last().map(|f| f.path.clone());
        }
    }

    /// Set the active file by path.
    pub fn set_active_file(&mut self, path: impl Into<String>) {
        self.active_file_path = Some(path.into());
    }

    /// Update a specific open file by path using a closure.
    pub fn update_file(&mut self, path: &str, f: impl FnOnce(&mut EditorFile)) {
        if let Some(file) = self.open_files.iter_mut().find(|f_| f_.path == path) {
            f(file);
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the editor signal from the Dioxus context.
pub fn use_editor_store() -> Signal<EditorState> {
    use_context::<Signal<EditorState>>()
}

/// Initialize the editor store as a context provider.
pub fn provide_editor_store() {
    use_context_provider(|| Signal::new(EditorState::new()));
}
