//! Global keyboard shortcut classification.
//!
//! This module deliberately contains no signals, hooks, DOM access, or state
//! mutation. It only turns a key/modifier pair into a named application action;
//! [`crate::App`] remains responsible for executing that action.

use dioxus::prelude::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKeyAction {
    ToggleCommandPalette,
    ToggleRightSidebar,
    ShowNewSpace,
    ToggleSidebar,
    SetWorkspacePanel,
    SetEditorPanel,
    SetKanbanPanel,
    SetSwarmPanel,
    CloseFirstPane,
    ToggleEditorPanel,
    ShowSettings,
    ShowSwarmModal,
    AddShell,
    ResetWorkspaceView,
    Escape,
}

/// Classify a global shortcut without performing its action.
pub fn classify(key: &Key, modifiers: Modifiers) -> Option<GlobalKeyAction> {
    // Escape remains global even when modifier keys are held, matching the
    // original App handler's unconditional Escape branch.
    if matches!(key, Key::Escape) {
        return Some(GlobalKeyAction::Escape);
    }

    let meta = modifiers.contains(Modifiers::META) || modifiers.contains(Modifiers::CONTROL);
    let shift = modifiers.contains(Modifiers::SHIFT);

    if meta && !shift {
        return match key {
            Key::Character(c) if c == "k" || c == "p" => {
                Some(GlobalKeyAction::ToggleCommandPalette)
            }
            Key::Character(c) if c == "j" || c == "\\" => Some(GlobalKeyAction::ToggleRightSidebar),
            Key::Character(c) if c == "t" => Some(GlobalKeyAction::ShowNewSpace),
            Key::Character(c) if c == "b" => Some(GlobalKeyAction::ToggleSidebar),
            Key::Character(c) if c == "1" => Some(GlobalKeyAction::SetWorkspacePanel),
            Key::Character(c) if c == "2" => Some(GlobalKeyAction::SetEditorPanel),
            Key::Character(c) if c == "3" => Some(GlobalKeyAction::SetKanbanPanel),
            Key::Character(c) if c == "4" => Some(GlobalKeyAction::SetSwarmPanel),
            Key::Character(c) if c == "w" => Some(GlobalKeyAction::CloseFirstPane),
            Key::Character(c) if c == "e" => Some(GlobalKeyAction::ToggleEditorPanel),
            Key::Character(c) if c == "," => Some(GlobalKeyAction::ShowSettings),
            _ => None,
        };
    }

    if meta && shift {
        return match key {
            Key::Character(c) if c == "S" => Some(GlobalKeyAction::ShowSwarmModal),
            Key::Character(c) if c == "P" => Some(GlobalKeyAction::ToggleCommandPalette),
            Key::Character(c) if c == "A" => Some(GlobalKeyAction::AddShell),
            Key::Character(c) if c == "R" => Some(GlobalKeyAction::ResetWorkspaceView),
            _ => None,
        };
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> Modifiers {
        Modifiers::META
    }

    #[test]
    fn classifies_panel_shortcuts() {
        assert_eq!(
            classify(&Key::Character("1".into()), meta()),
            Some(GlobalKeyAction::SetWorkspacePanel)
        );
        assert_eq!(
            classify(&Key::Character("2".into()), meta()),
            Some(GlobalKeyAction::SetEditorPanel)
        );
        assert_eq!(
            classify(&Key::Character("4".into()), meta()),
            Some(GlobalKeyAction::SetSwarmPanel)
        );
    }

    #[test]
    fn classifies_shift_shortcuts() {
        assert_eq!(
            classify(
                &Key::Character("A".into()),
                Modifiers::META | Modifiers::SHIFT
            ),
            Some(GlobalKeyAction::AddShell)
        );
        assert_eq!(
            classify(
                &Key::Character("S".into()),
                Modifiers::META | Modifiers::SHIFT
            ),
            Some(GlobalKeyAction::ShowSwarmModal)
        );
    }

    #[test]
    fn escape_is_global_with_or_without_modifiers() {
        for modifiers in [
            Modifiers::empty(),
            Modifiers::META,
            Modifiers::META | Modifiers::SHIFT,
        ] {
            assert_eq!(
                classify(&Key::Escape, modifiers),
                Some(GlobalKeyAction::Escape)
            );
        }
    }

    #[test]
    fn unrelated_key_is_ignored() {
        assert_eq!(classify(&Key::Character("x".into()), Modifiers::META), None);
    }
}
