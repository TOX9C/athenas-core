//! Resume-ID scanning for agent PTY output.
//!
//! Both the stateful [`ResumeScanner`] (rolling buffer + dedup) and the
//! stateless [`scan_text_for_resume_id`] helper are owned by the shared,
//! dependency-free `athena-resume-scanner` crate so the frontend (WASM) and
//! backend cannot drift on parsing rules. This module re-exports them to
//! preserve the historical `athena_core::resume_scanner::*` paths.
//!
//! The backend's app-exit capture path consumes the stateless helper over an
//! accumulated output snapshot (see `src-tauri/src/commands/resume.rs`).

pub use athena_resume_scanner::{scan_text_for_resume_id, ResumeScanner};
