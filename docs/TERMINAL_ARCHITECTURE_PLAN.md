# Terminal Architecture Plan — Athena's Core

> Modeled after Warp's terminal architecture. All xterm.js removed. Native Rust terminal with Tauri IPC → Dioxus WASM rendering.

---

## 1. Architecture Overview

```
User Input (keyboard, mouse)
    ┃
    ┣━→ Tauri Window listens on keyboard/mouse events
    ┃       ┃
    ┃       ▼
    ┃   frontend/src/components/terminal/terminal_pane.rs
    ┃       │ Encodes keystrokes to ANSI escape sequences
    ┃       │ via `escape_sequences.rs` (Kitty protocol + xterm legacy)
    ┃       │
    ┃       ▼
    ┃   frontend/src/tauri_bridge.rs  ──invoke──►  Tauri backend PTY command
    ┃                           (pty_write)
    ┃
    ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━→  PTY master fd  ←── Shell child process
                                                                  (zsh, bash, etc.)
                                                     ┃
                                                     ▼
                                              read() raw bytes
                                                     ┃
                                                     ▼
                                              vte crate (ANSI parser)
                                                     ┃
                                                     ▼
                                              Grid (Cell / Row) mutation
                                                     ┃
                                                     ▼
                                              Tauri event emit (batch deltas)
                                                     ┃
                                                     ▼
                                              frontend store update
                                                     ┃
                                                     ▼
                                              Dioxus re-render → HTML <pre> grid
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **No xterm.js** | JavaScript terminal emulator had untraceable input bugs in WASM bridge. Rust-native `vte` is battle-tested. |
| **HTML grid rendering, not Canvas** | Dioxus/WASM target. CSS grid of `<span>` elements. True GPU rendering (Warp's wgpu) is future phase. |
| **vte crate for parsing** | Same crate as Warp/Alacritty. VT100/ANSI escape sequence parser in pure Rust. |
| **Cell grid with dirty tracking** | Only re-render changed cells. Avoids full DOM redraw on every PTY byte. |
| **Escape sequence encoding in frontend** | Keyboard → ANSI happens in Dioxus (WASM). Sends raw bytes to backend via `pty_write`. |
| **Batch delta emission over IPC** | Backend emits cell-change deltas as Tauri events, not full grid. Frontend applies to store. |
| **Scrollback in flat storage (Phase 2)** | Initial: all rows in memory. Later: compress to flat string + attribute maps. |
| **Tauri `stdio` or `tauri-plugin-shell`** | PTY spawning. macOS/Linux via `libc::openpty()` + `nix`. Windows via `conpty`. |

---

## 2. Crate Structure

Add back `crates/athena-terminal/` with this layout:

```
crates/
  athena-terminal/
    Cargo.toml
    src/
      lib.rs                      -- Re-exports (TerminalSession, Grid, AnsiHandler)
      session.rs                  -- PTY spawn, kill, resize; async read loop; event emit
      grid/
        mod.rs                    -- Grid: rows, cols, scrollback, cursor, dirty tracking
        cell.rs                   -- Cell struct (char, fg, bg, flags, optional CellExtra)
        row.rs                     -- Row: Vec<Cell>, occ (occupancy), dirty range
        colors.rs                  -- Color enum (Named, Indexed, Rgb, Default)
        flat_storage.rs            -- Phase 2: compressed scrollback (flat string + attr maps)
      ansi/
        mod.rs                     -- Re-exports
        handler.rs                 -- VTE Perform impl: maps ANSI ops → Grid mutations
        escape_sequences.rs        -- Keystroke → ANSI bytes (Kitty + xterm)
        mode.rs                     -- TermMode bitflags
      input/
        mod.rs                     -- Keyboard/mouse → ANSI encoding
        kitty.rs                   -- Kitty keyboard protocol
    tests/
      grid_tests.rs
      ansi_tests.rs
```

Add terminal rendering in frontend:

```
frontend/src/
  components/
    terminal/
      mod.rs                     -- pub mod terminal_pane, terminal_grid
      terminal_pane.rs           -- Focus, keydown, onclick, sends input to tauri_bridge
      terminal_grid.rs           -- Renders Grid as CSS <pre> grid from store
  stores/
    terminal.rs                  -- TerminalStore: holds Grid state, applies deltas
  tauri_bridge.rs              -- Add pty_spawn, pty_write, pty_kill, pty_resize
```

Add PTY commands back to Tauri:

```
src-tauri/src/
  commands/
    pty.rs                       -- Real PTY commands (spawn, write, kill, resize, etc.)
  state.rs                      -- Add pty_manager: Arc<Mutex<PtySessionManager>>
  main.rs                       -- Wire pty_spawn, pty_write, etc. into invoke_handler
```

---

## 3. Data Types

### 3.1 Cell (24 bytes target)

```rust
// crates/athena-terminal/src/grid/cell.rs
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub c: char,                          // 4 bytes
    pub fg: Color,                        // Color enum
    pub bg: Color,
    pub flags: CellFlags,                 // bitflags (bold, italic, inverse, etc.)
    extra: Option<Box<CellExtra>>,        // Lazy allocation
}

bitflags::bitflags! {
    pub struct CellFlags: u16 {
        const INVERSE      = 0b0000_0001;
        const BOLD         = 0b0000_0010;
        const ITALIC       = 0b0000_0100;
        const UNDERLINE    = 0b0000_1000;
        const WRAPLINE     = 0b0001_0000;
        const WIDE_CHAR    = 0b0010_0000;
        const DIM          = 0b1000_0000;
        const STRIKEOUT    = 0b0010_0000_0000;
    }
}

struct CellExtra {
    zero_width_chars: Option<String>,     // Combining marks
}
```

### 3.2 Row

```rust
// crates/athena-terminal/src/grid/row.rs
#[derive(Clone, Debug, Default)]
pub struct Row {
    inner: Vec<Cell>,
    pub occ: usize,           // Occupancy: last non-empty cell index + 1
    dirty_start: Option<usize>,
    dirty_end: Option<usize>,
}

impl Row {
    pub fn new(cols: usize) -> Self { /* ... */ }
    pub fn grow(&mut self, cols: usize) { /* ... */ }
    pub fn shrink(&mut self, cols: usize) -> Option<Vec<Cell>> { /* ... */ }
    pub fn reset(&mut self, template: &Cell) { /* ... */ }
    pub fn dirty_range(&self) -> Option<(usize, usize)> { /* ... */ }
    pub fn clear_dirty(&mut self) { /* ... */ }
}
```

### 3.3 Grid

```rust
// crates/athena-terminal/src/grid/mod.rs
pub struct Grid {
    rows: Vec<Row>,
    scrollback: Vec<Row>,               // Phase 1: simple Vec. Phase 2: FlatStorage
    cursor: Point,                      // (row, col)
    saved_cursor: Option<Point>,
    cols: usize,
    rows_count: usize,
    mode: TermMode,
    scroll_region: (usize, usize),      // Top/bottom for scroll
    selection: Option<Selection>,
    dirty_regions: Vec<(usize, usize, usize, usize)>, // Track dirty for batch emit
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self;
    pub fn insert_char(&mut self, c: char);
    pub fn delete_chars(&mut self, count: usize);
    pub fn scroll_up(&mut self, lines: usize);
    pub fn scroll_down(&mut self, lines: usize);
    pub fn resize(&mut self, cols: usize, rows: usize);
    pub fn dirty_deltas(&self) -> Vec<CellDelta>;
    pub fn clear_dirty(&mut self);
}
```

### 3.4 TerminalSession (PTY + Grid wrapper)

```rust
// crates/athena-terminal/src/session.rs
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct TerminalSession {
    pub id: String,
    pub grid: Arc<Mutex<Grid>>,
    pub pty: PtyHandle,              // Platform-specific: fd on Unix, HANDLE on Windows
    pub process: ChildProcess,
    pub shell: ShellInfo,            // Shell type, version, etc.
}

pub struct PtyHandle {
    #[cfg(unix)]
    pub master: i32,                 // PTY master fd
    #[cfg(windows)]
    pub handle: HANDLE,              // ConPTY handle
}
```

---

## 4. PTY Operations

### 4.1 Spawn (Unix)

```rust
// crates/athena-terminal/src/session.rs
use nix::pty::{openpty, Winsize};
use nix::unistd::{fork, ForkResult, execve, close};
use std::os::unix::io::RawFd;

pub fn spawn_pty(cols: u16, rows: u16, shell: &str, cwd: &str) -> io::Result<(RawFd, Child)> {
    let winsize = Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None)?;
    let master_fd = pty.master;
    let slave_fd = pty.slave;

    match unsafe { fork() }? {
        ForkResult::Child => {
            close(master_fd)?;
            // Create new session, set controlling tty
            nix::unistd::setsid()?;
            // Duplicate slave fd to stdin/stdout/stderr
            dup2(slave_fd, 0)?;
            dup2(slave_fd, 1)?;
            dup2(slave_fd, 2)?;
            close(slave_fd)?;
            // Exec shell
            let shell_cstr = CString::new(shell)?;
            let args: Vec<CString> = vec![shell_cstr.clone()];
            let envp: Vec<CString> = vec![]; // Inherit env
            execve(&shell_cstr, &args, &envp)?;
            unreachable!()
        }
        ForkResult::Parent { child } => {
            close(slave_fd)?;
            Ok((master_fd, Child::new(child)))
        }
    }
}
```

### 4.2 Read Loop (async)

```rust
// crates/athena-terminal/src/session.rs
use tokio::io::{AsyncReadExt, Interest};
use vte::{Parser, Perform};

pub async fn read_loop(
    master_fd: RawFd,
    grid: Arc<tokio::sync::Mutex<Grid>>,
    app_handle: tauri::AppHandle,
    session_id: String,
) -> io::Result<()> {
    let mut buf = [0u8; 8192];
    let mut parser = Parser::new();
    let handler = AnsiHandler::new(grid.clone());

    loop {
        let n = tokio::task::spawn_blocking({
            let fd = master_fd;
            move || {
                // Read from PTY master (blocking)
                let mut buf = [0u8; 4096];
                let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok((buf, n as usize))
                }
            }
        }).await.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        match n {
            Ok((buf, 0)) => break, // EOF
            Ok((buf, len)) => {
                let bytes = &buf[..len];
                parser.parse(bytes, &handler);
                // Emit deltas after each parse chunk
                let deltas = {
                    let g = grid.lock().await;
                    g.dirty_deltas()
                };
                if !deltas.is_empty() {
                    app_handle.emit(&format!("terminal:update:{}", session_id), &deltas)
                        .ok();
                }
                {
                    let mut g = grid.lock().await;
                    g.clear_dirty();
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
```

### 4.3 Write to PTY

```rust
// Tauri command (src-tauri/src/commands/pty.rs)
#[tauri::command]
pub async fn pty_write(
    state: State<'_, AppState>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    state.pty_manager.write(&id, &data).await.map_err(|e| e.to_string())
}
```

### 4.4 Resize

```rust
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.pty_manager.resize(&id, cols, rows).await.map_err(|e| e.to_string())
}
```

---

## 5. Escape Sequence Handling (VTE)

```rust
// crates/athena-terminal/src/ansi/handler.rs
use vte::{Perform, Params};

pub struct AnsiHandler {
    grid: Arc<tokio::sync::Mutex<Grid>>,
}

impl Perform for AnsiHandler {
    fn print(&mut self, c: char) {
        let mut grid = self.grid.blocking_lock();
        grid.insert_char(c);
    }

    fn execute(&mut self, byte: u8) {
        let mut grid = self.grid.blocking_lock();
        match byte {
            0x07 => { /* BEL - bell, nop */ }
            0x08 => grid.move_cursor_left(1),       // Backspace
            0x09 => grid.tab(),                      // Tab
            0x0A => grid.newline(),                  // Line feed
            0x0D => grid.carriage_return(),          // CR
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        let mut grid = self.grid.blocking_lock();
        match action {
            'A' => grid.move_cursor_up(params.iter().next().unwrap_or(1)),       // CUU
            'B' => grid.move_cursor_down(params.iter().next().unwrap_or(1)),    // CUD
            'C' => grid.move_cursor_right(params.iter().next().unwrap_or(1)),   // CUF
            'D' => grid.move_cursor_left(params.iter().next().unwrap_or(1)),    // CUB
            'E' => grid.move_cursor_down_and_home(params.iter().next().unwrap_or(1)), // CNL
            'F' => grid.move_cursor_up_and_home(params.iter().next().unwrap_or(1)),   // CPL
            'G' => grid.move_cursor_to_column(params.iter().next().unwrap_or(1)),       // CHA
            'H' => { // CUP
                let (row, col) = parse_two_params(params, 1, 1);
                grid.move_cursor_to(row - 1, col - 1);
            }
            'J' => { // ED - erase display
                let mode = params.iter().next().unwrap_or(0);
                grid.erase_display(mode);
            }
            'K' => { // EL - erase line
                let mode = params.iter().next().unwrap_or(0);
                grid.erase_line(mode);
            }
            'm' => { // SGR - select graphic rendition
                let params: Vec<u16> = params.iter().collect();
                grid.set_sgr(&params);
            }
            // ... handle more CSI sequences
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        // OSC sequences (window title, color queries, etc.)
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        // ESC sequences
    }
}
```

---

## 6. Input Encoding (Frontend → ANSI)

```rust
// frontend/src/components/terminal/terminal_pane.rs
// Keyboard event handler

fn onkeydown(event: KeyboardEvent, session_id: String) {
    let key = event.key();
    let ctrl = event.ctrl_key();
    let alt = event.alt_key();
    let shift = event.shift_key();
    let meta = event.meta_key();

    // Encode to ANSI escape sequence
    let bytes = encode_keystroke(&key, ctrl, alt, shift, meta);
    
    // Send to backend via Tauri bridge
    spawn(async move {
        let _ = tauri_bridge::pty_write(&session_id, &bytes).await;
    });
}

fn encode_keystroke(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Vec<u8> {
    use crate::terminal::input::escape_sequences::*;
    
    // 1. Kitty keyboard protocol (CSI u)
    if let Some(csi_u) = kitty_encode(key, ctrl, alt, shift, meta) {
        return csi_u;
    }
    
    // 2. Legacy xterm encoding
    if ctrl && key.len() == 1 {
        let c = key.chars().next().unwrap();
        let byte = c as u8;
        if byte >= b'a' && byte <= b'z' {
            return vec![byte - b'a' + 1]; // Ctrl+A = x01, etc.
        }
    }
    
    // 3. Special keys
    match key {
        "Enter" => vec![0x0D],           // CR
        "Tab" => vec![0x09],             // HT
        "Backspace" => vec![0x7F],       // DEL
        "Escape" => vec![0x1B],          // ESC
        "ArrowUp" => vec![0x1B, b'[', b'A'],
        "ArrowDown" => vec![0x1B, b'[', b'B'],
        "ArrowRight" => vec![0x1B, b'[', b'C'],
        "ArrowLeft" => vec![0x1B, b'[', b'D'],
        "Home" => vec![0x1B, b'[', b'H'],
        "End" => vec![0x1B, b'[', b'F'],
        "PageUp" => vec![0x1B, b'[', b'5', b'~'],
        "PageDown" => vec![0x1B, b'[', b'6', b'~'],
        "Delete" => vec![0x1B, b'[', b'3', b'~'],
        _ => {
            // Regular character
            if key.len() == 1 {
                key.bytes().collect()
            } else {
                vec![]
            }
        }
    }
}
```

---

## 7. Frontend Rendering

### 7.1 TerminalStore (Frontend State)

```rust
// frontend/src/stores/terminal.rs
use std::collections::HashMap;
use dioxus::prelude::*;

pub struct TerminalStore {
    pub sessions: HashMap<String, TerminalSessionState>,
    pub active_session: Option<String>,
}

pub struct TerminalSessionState {
    pub id: String,
    pub grid: GridState,            // Serializable subset of backend Grid
    pub cols: usize,
    pub rows: usize,
    pub cursor: (usize, usize),
    pub scrollback_offset: usize,
}

pub struct GridState {
    pub rows: Vec<RowState>,
    pub dirty: Vec<(usize, usize, usize, usize)>,  // (start_row, start_col, end_row, end_col)
}

pub struct RowState {
    pub cells: Vec<CellState>,
    pub occ: usize,
}

#[derive(Clone, Debug)]
pub struct CellState {
    pub c: char,
    pub fg: ColorState,
    pub bg: ColorState,
    pub flags: u16,
}

impl TerminalStore {
    pub fn apply_deltas(&mut self, session_id: &str, deltas: Vec<CellDelta>) {
        // Apply backend deltas to frontend GridState
    }
    
    pub fn scroll_up(&mut self, lines: usize) {
        // Scrollback navigation
    }
}
```

### 7.2 TerminalGrid Component

```rust
// frontend/src/components/terminal/terminal_grid.rs
// Renders the terminal grid as HTML/CSS for Dioxus WASM

#[component]
pub fn TerminalGrid(session_id: String) -> Element {
    let store = use_terminal_store();
    let session = store.read().get_session(&session_id);
    
    rsx! {
        div {
            class: "terminal-grid",
            style: "font-family: 'JetBrains Mono', monospace; font-size: 13px; line-height: 1.4; white-space: pre; overflow: auto; background: var(--bg); color: var(--text); padding: 4px;",
            
            for (row_idx, row) in session.grid.rows.iter().enumerate() {
                div {
                    class: "terminal-row",
                    style: "display: flex; min-height: 1.4em;",
                    key: "{row_idx}",
                    
                    for (col_idx, cell) in row.cells.iter().enumerate() {
                        if col_idx < row.occ {
                            span {
                                key: "{col_idx}",
                                style: cell_style(cell),
                                "{cell.c}"
                            }
                        }
                    }
                }
            }
            
            // Cursor
            if session.cursor.0 < session.grid.rows.len() {
                Cursor { 
                    row: session.cursor.0, 
                    col: session.cursor.1 
                }
            }
        }
    }
}

fn cell_style(cell: &CellState) -> String {
    let mut styles = Vec::new();
    
    if cell.flags & FLAG_BOLD != 0 { styles.push("font-weight: bold;".to_string()); }
    if cell.flags & FLAG_ITALIC != 0 { styles.push("font-style: italic;".to_string()); }
    if cell.flags & FLAG_UNDERLINE != 0 { styles.push("text-decoration: underline;".to_string()); }
    if cell.flags & FLAG_STRIKEOUT != 0 { styles.push("text-decoration: line-through;".to_string()); }
    if cell.flags & FLAG_INVERSE != 0 { 
        styles.push("filter: invert(1);".to_string()); 
    }
    
    match &cell.fg {
        ColorState::Named(name) => styles.push(format!("color: var(--color-{});", named_color_css(name))),
        ColorState::Rgb(r, g, b) => styles.push(format!("color: rgb({},{},{});", r, g, b)),
        _ => {}
    }
    
    match &cell.bg {
        ColorState::Named(name) => styles.push(format!("background: var(--color-{});", named_color_css(name))),
        ColorState::Rgb(r, g, b) => styles.push(format!("background: rgb({},{},{});", r, g, b)),
        _ => {}
    }
    
    styles.join(" ")
}
```

---

## 8. Tauri Commands

```rust
// src-tauri/src/commands/pty.rs
use athena_terminal::{TerminalSession, PtyHandle};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Spawn a new PTY session.
#[tauri::command]
pub async fn pty_spawn(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.pty_manager.spawn(&id, &cwd, &shell, cols, rows).await
        .map_err(|e| e.to_string())
}

/// Write raw bytes to a PTY session.
#[tauri::command]
pub async fn pty_write(
    state: State<'_, AppState>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    state.pty_manager.write(&id, &data).await
        .map_err(|e| e.to_string())
}

/// Send a resize request to a PTY session.
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.pty_manager.resize(&id, cols, rows).await
        .map_err(|e| e.to_string())
}

/// Kill a PTY session.
#[tauri::command]
pub async fn pty_kill(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.pty_manager.kill(&id).await
        .map_err(|e| e.to_string())
}

/// Check if a PTY session exists.
#[tauri::command]
pub async fn pty_has_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    Ok(state.pty_manager.has_session(&id).await)
}

/// Get session history (scrollback buffer as text).
#[tauri::command]
pub async fn pty_get_history(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    state.pty_manager.get_history(&id).await
        .map_err(|e| e.to_string())
}

/// Check if a PTY session is ready (shell prompt visible).
#[tauri::command]
pub async fn pty_is_ready(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    Ok(state.pty_manager.is_ready(&id).await)
}

/// Get the current working directory of a PTY session.
#[tauri::command]
pub async fn pty_get_cwd(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, String> {
    state.pty_manager.get_cwd(&id).await
        .map_err(|e| e.to_string())
}

/// Spawn a PTY session and immediately execute an agent command.
#[tauri::command]
pub async fn pty_spawn_agent(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    agent_cmd: String,
) -> Result<(), String> {
    state.pty_manager.spawn_agent(&id, &cwd, &shell, &agent_cmd).await
        .map_err(|e| e.to_string())
}

/// Get the default shell path.
#[tauri::command]
pub fn pty_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") { "powershell.exe".to_string() } 
        else { "/bin/zsh".to_string() }
    })
}
```

---

## 9. Frontend-Backend IPC Data Flow

```
User presses "ls" + Enter
    ┃
    ▼
TerminalPane::onkeydown("Enter")
    ┃ Encode to [0x0D] (CR)
    ▼
tauri_bridge::pty_write(session_id, [0x0D])
    ┃ invoke("pty_write", {id, data: Uint8Array})
    ▼
Tauri backend → pty_write command
    ┃ libc::write(master_fd, [0x0D])
    ▼
Shell receives "\n", executes "ls", writes output
    ┃
    ┗━→ PTY slave → master fd → read_loop()
                                ┃
                                ▼
                            vte::Parser
                                ┃
                                ▼
                            AnsiHandler (update Grid)
                                ┃
                                ▼
                            Grid.dirty_deltas()
                                ┃
                                ▼
                            app_handle.emit("terminal:update:{id}", [cell deltas])
                                ┃
                                ▼
                            Frontend TerminalStore::apply_deltas()
                                ┃
                                ▼
                            Dioxus signal triggers re-render
                                ┃
                                ▼
                            TerminalGrid component renders updated cells
```

---

## 10. Implementation Phases

### Phase 1: Core Terminal (crates/athena-terminal)
- [ ] Create `crates/athena-terminal/` crate
- [ ] Implement `grid/cell.rs` (Cell, CellFlags, Color)
- [ ] Implement `grid/row.rs` (Row with occ/dirty tracking)
- [ ] Implement `grid/mod.rs` (Grid: insert, scroll, resize, cursor ops)
- [ ] Implement `ansi/handler.rs` (VTE Perform trait)
- [ ] Implement `session.rs` (PTY spawn, async read loop, event emit)
- [ ] Unit tests for Grid ops and ANSI parsing

### Phase 2: Tauri Backend Integration
- [ ] Wire `pty_spawn`, `pty_write`, `pty_kill`, `pty_resize` in `commands/pty.rs`
- [ ] Add `pty_manager` to `AppState`
- [ ] Register commands in `main.rs` `generate_handler!`
- [ ] Test: spawn shell, type command, see output via Tauri events

### Phase 3: Frontend Store & Bridge
- [ ] Add `pty_write` etc. to `tauri_bridge.rs`
- [ ] Implement `TerminalStore` with delta application
- [ ] Listen for `terminal:update:{id}` Tauri events

### Phase 4: Rendering
- [ ] Create `TerminalPane` component (focus, key capture)
- [ ] Create `TerminalGrid` component (CSS grid rendering)
- [ ] Implement cursor rendering
- [ ] Basic color support (256 colors + RGB)

### Phase 5: Polish
- [ ] Selection (click-drag, copy)
- [ ] Scrollback via scroll offset
- [ ] Scrollback compression (FlatStorage)
- [ ] Performance: frame coalescing, delta only
- [ ] Mouse support (OSC 1005/1006)
- [ ] Shell integration (OSC 133 prompt markers)
- [ ] Windows ConPTY support

---

## 11. Key Files to Create / Modify

| File | Action | Purpose |
|------|--------|---------|
| `crates/athena-terminal/Cargo.toml` | **Create** | New crate with vte, nix, tokio deps |
| `crates/athena-terminal/src/lib.rs` | **Create** | Public API |
| `crates/athena-terminal/src/session.rs` | **Create** | PTY + read loop |
| `crates/athena-terminal/src/grid/` | **Create** | Cell, Row, Grid types |
| `crates/athena-terminal/src/ansi/` | **Create** | VTE handler, escape seqs |
| `src-tauri/src/commands/pty.rs` | **Restore** | Real PTY commands |
| `src-tauri/src/state.rs` | **Modify** | Add `pty_manager` field |
| `src-tauri/src/main.rs` | **Modify** | Register pty commands |
| `src-tauri/Cargo.toml` | **Modify** | Add `athena-terminal` dep |
| `src-tauri/build.rs` | **Modify** | Add pty commands to COMMANDS array |
| `src-tauri/capabilities/default.json` | **Modify** | Add pty permissions |
| `frontend/src/tauri_bridge.rs` | **Modify** | Add pty functions |
| `frontend/src/stores/terminal.rs` | **Create** | TerminalStore |
| `frontend/src/components/terminal/` | **Create** | TerminalPane, TerminalGrid |
| `Cargo.toml` (root) | **Modify** | Add athena-terminal to workspace |
