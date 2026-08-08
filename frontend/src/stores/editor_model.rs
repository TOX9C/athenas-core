//! Pure editor data contracts.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty() {
        assert_eq!(
            CursorPosition::default(),
            CursorPosition { line: 0, column: 0 }
        );
        assert_eq!(
            EditorFile::default(),
            EditorFile {
                path: String::new(),
                content: String::new(),
                language: String::new(),
                is_dirty: false,
                cursor_position: CursorPosition::default(),
            }
        );
    }

    #[test]
    fn editor_file_preserves_cursor_contract() {
        let file = EditorFile {
            path: "src/main.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: "rust".to_string(),
            is_dirty: true,
            cursor_position: CursorPosition { line: 4, column: 7 },
        };
        assert_eq!(file.cursor_position.line, 4);
        assert_eq!(file.cursor_position.column, 7);
        assert!(file.is_dirty);
    }
}
