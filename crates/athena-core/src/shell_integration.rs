//! Shell integration module — ported from electron/services/shellIntegration.ts
//!
//! Provides OSC 633 parsing, command tracking, and shell integration scripts
//! for zsh, bash, and fish.

#[path = "shell_integration_parser.rs"]
mod shell_integration_parser;
#[path = "shell_integration_tracker.rs"]
mod shell_integration_tracker;
pub use shell_integration_parser::{
    parse_osc633, strip_osc633, Osc633Parser, ParsedSequence, ShellIntegrationSequence,
};
pub use shell_integration_tracker::{
    create_command_tracker, process_sequences, CommandTracker, ShellIntegrationEvent,
};
#[path = "shell_integration_scripts.rs"]
mod shell_integration_scripts;
pub use shell_integration_scripts::{
    build_shell_integration_env, get_shell_integration_script, is_shell_integration_compatible,
    ShellIntegrationError,
};

#[cfg(test)]
mod strip_osc633_tests {
    use super::*;

    #[test]
    fn strip_osc633_removes_bel_terminator() {
        let input = "\x1b]633;set-mark\x07hello";
        let output = strip_osc633(input);
        assert_eq!(output, "hello");
        assert!(!output.contains('\x07'));
    }

    #[test]
    fn strip_osc633_removes_st_terminator() {
        let input = "\x1b]633;set-mark\x1b\\hello";
        let output = strip_osc633(input);
        assert_eq!(output, "hello");
        assert!(!output.contains("\x1b\\"));
    }

    #[test]
    fn strip_osc633_preserves_non_osc_text() {
        let input = "regular text";
        let output = strip_osc633(input);
        assert_eq!(output, "regular text");
    }
}

#[cfg(test)]
mod get_shell_integration_script_tests {
    use super::*;

    #[test]
    fn known_shells_return_ok() {
        assert!(get_shell_integration_script("bash").is_ok());
        assert!(get_shell_integration_script("zsh").is_ok());
        assert!(get_shell_integration_script("fish").is_ok());
    }

    #[test]
    fn known_shells_with_paths_return_ok() {
        // Full paths should resolve to the same scripts as bare names.
        assert!(get_shell_integration_script("/bin/bash").is_ok());
        assert!(get_shell_integration_script("/usr/local/bin/zsh").is_ok());
        assert!(get_shell_integration_script("/opt/homebrew/bin/fish").is_ok());
    }

    #[test]
    fn known_shells_return_real_script() {
        // Sanity-check the script content is shell-specific, not a zsh fallback.
        let zsh = get_shell_integration_script("zsh").unwrap();
        assert!(zsh.contains("add-zsh-hook"));
        let bash = get_shell_integration_script("bash").unwrap();
        assert!(bash.contains("PROMPT_COMMAND"));
        let fish = get_shell_integration_script("fish").unwrap();
        assert!(fish.contains("fish_prompt"));
    }

    #[test]
    fn unknown_shell_returns_unsupported_error() {
        let result = get_shell_integration_script("tcsh");
        assert!(
            matches!(&result, Err(ShellIntegrationError::UnsupportedShell(s)) if s == "tcsh"),
            "expected UnsupportedShell(\"tcsh\"), got {result:?}"
        );
    }

    #[test]
    fn unknown_shell_with_path_returns_basename_in_error() {
        // The error reports the basename of the unsupported path.
        let result = get_shell_integration_script("/usr/bin/tcsh");
        assert!(
            matches!(&result, Err(ShellIntegrationError::UnsupportedShell(s)) if s == "tcsh"),
            "expected UnsupportedShell(\"tcsh\"), got {result:?}"
        );
    }

    #[test]
    fn empty_shell_returns_unsupported_error() {
        let result = get_shell_integration_script("");
        assert!(
            matches!(&result, Err(ShellIntegrationError::UnsupportedShell(s)) if s.is_empty()),
            "expected UnsupportedShell with empty name, got {result:?}"
        );
    }

    #[test]
    fn unsupported_shell_does_not_silently_fallback_to_zsh() {
        // Regression: prior implementation returned the zsh script for any
        // unknown shell. The function must now return Err for unsupported
        // shells — no script is ever returned for them.
        let err = get_shell_integration_script("powershell").unwrap_err();
        match err {
            ShellIntegrationError::UnsupportedShell(name) => {
                assert_eq!(name, "powershell");
            }
        }
    }
}
