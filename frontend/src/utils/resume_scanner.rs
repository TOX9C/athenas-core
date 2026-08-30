//! Live-stream resume-ID scanning for agent PTY output.
//!
//! The stateful [`ResumeScanner`] (rolling buffer + dedup) is owned by the
//! shared, dependency-free `athena-resume-scanner` crate so the frontend and
//! backend cannot drift on parsing rules. This module re-exports it to
//! preserve the `crate::utils::resume_scanner::ResumeScanner` path consumed by
//! the xterm mount.
pub use athena_resume_scanner::ResumeScanner;
