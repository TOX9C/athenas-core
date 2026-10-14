//! PTY process management for Athena. The terminal emulator (cell grid, ANSI
//! parsing) is owned by the frontend's xterm.js instance; this crate manages
//! spawning, fd lifecycle, raw byte streaming, and shutdown.

pub mod session;
