# Contributing Guide

## Setting Up the Development Environment

### 1. Install Prerequisites

```bash
# Rust (1.82+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tauri CLI
cargo install tauri-cli

# Dioxus CLI
cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli

# macOS: Xcode Command Line Tools
xcode-select --install

# Linux (Debian/Ubuntu)
sudo apt install -y \
  webkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### 2. Clone and Verify

```bash
cd rust-migration
cargo check --workspace
cargo test --workspace
```

### 3. Run in Development Mode

```bash
# One-step (recommended)
./dev.sh

# Or step by step:
dx build --package athena-frontend
cp -r target/dx/athena-frontend/debug/web/public/* frontend/dist/
cargo tauri dev
```

## Running Tests

```bash
# All tests
cargo test --workspace

# Single crate
cargo test -p athena-core
cargo test -p athena-terminal
cargo test -p athena-store

# With verbose output
cargo test --workspace -- --nocapture

# Run a specific test
cargo test -p athena-core test_anthropic_message_format

# Run tests matching a pattern
cargo test --workspace -- --test-threads=1 search
```

## Code Style

### Formatting

All code must be formatted with `rustfmt`:

```bash
cargo fmt --all
```

Configuration is in `rustfmt.toml` at the workspace root.

### Clippy

All code must pass clippy without warnings:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Guidelines

- **Error handling:** Use `thiserror` for library errors, `CommandError` for Tauri commands. Never use `.unwrap()` in production paths.
- **Concurrency:** Use `Arc<Mutex<T>>` for sync code, `Arc<tokio::sync::Mutex<T>>` for async. Prefer `tokio::task::spawn_blocking` for blocking I/O.
- **Naming:** Follow Rust conventions: `snake_case` for functions/variables, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- **Imports:** Group imports: std, external crates, internal modules. Use `use crate::` for intra-workspace imports.
- **Documentation:** Add `///` doc comments to all public items.

## How to Add a New Tauri Command

1. **Define the command** in `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn my_new_command(
    state: State<'_, AppState>,
    param1: String,
    param2: Option<i32>,
) -> Result<String, CommandError> {
    // Access shared state
    let manager = state.pty_manager.lock().map_err(|e| CommandError::Internal(e.to_string()))?;

    // Do work (use spawn_blocking for blocking operations)
    let result = tokio::task::spawn_blocking(move || {
        // blocking work
        Ok("result".to_string())
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Task failed: {e}")))?
    ?;

    Ok(result)
}
```

2. **Register the command** in `main.rs`'s `invoke_handler`:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands
    my_new_command,
])
```

3. **Add the frontend bridge** in `frontend/src/tauri_bridge.rs`:

```rust
pub async fn my_new_command(param1: String, param2: Option<i32>) -> Result<String, String> {
    invoke("my_new_command", InvokeArgs {
        param1,
        param2,
    }).await
}
```

4. **Emit events** (if the command triggers UI updates):

```rust
let app = state.app_handle.lock().ok().and_then(|h| h.clone());
if let Some(handle) = app {
    let _ = handle.emit("my:event", payload);
}
```

## How to Add a New Dioxus Component

1. **Create the component file** in the appropriate `frontend/src/components/` subdirectory:

```rust
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct MyComponentProps {
    pub title: String,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn MyComponent(props: MyComponentProps) -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        div {
            class: "my-component",
            h2 { "Title: {props.title}" }
            p { "Count: {count}" }
            button {
                onclick: move |_| count += 1,
                "Increment"
            }
            button {
                onclick: move |_| props.on_close.call(()),
                "Close"
            }
        }
    }
}
```

2. **Export from the module** in `frontend/src/components/mod.rs`:

```rust
pub mod my_component;
pub use my_component::MyComponent;
```

3. **Use in a parent component**:

```rust
use crate::components::MyComponent;

rsx! {
    MyComponent {
        title: "Hello",
        on_close: move |_| { /* handle close */ },
    }
}
```

## How to Add a New Store

1. **Create the store file** in `frontend/src/stores/`:

```rust
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct MyStoreData {
    pub items: Vec<String>,
    pub active_id: Option<String>,
}

pub fn provide_my_store() {
    provide_context(use_signal(|| MyStoreData {
        items: Vec::new(),
        active_id: None,
    }));
}

pub fn use_my_store() -> UseSignal<MyStoreData> {
    consume_context()
}
```

2. **Provide the store** in `App` component (`frontend/src/lib.rs`):

```rust
provide_my_store();
```

3. **Use in components**:

```rust
let store = use_my_store();
let items = store.read().items.clone();
```

## How to Add a New Crate

1. **Create the crate directory**:

```bash
mkdir -p crates/athena-new-crate/src
```

2. **Add `Cargo.toml`**:

```toml
[package]
name = "athena-new-crate"
version = "0.1.0"
edition = "2021"
rust-version = "1.82"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

3. **Add to workspace** in root `Cargo.toml`:

```toml
members = [
    # ... existing members
    "crates/athena-new-crate",
]
```

4. **Add as dependency** in `src-tauri/Cargo.toml` (if needed):

```toml
athena-new-crate = { path = "../crates/athena-new-crate" }
```

## Pull Request Process

1. **Create a feature branch** from `main`:

```bash
git checkout -b feature/my-feature
```

2. **Make your changes** and ensure:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo check --workspace
```

3. **Commit with a descriptive message**:

```bash
git add .
git commit -m "feat: add my new feature

- Description of what was added
- Why it was needed
- Any breaking changes"
```

4. **Open a PR** with:
   - Clear title following conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`)
   - Description of changes
   - Screenshots for UI changes
   - Test results

## Branch Naming

- `feature/description` — New features
- `fix/description` — Bug fixes
- `refactor/description` — Code refactoring
- `docs/description` — Documentation
- `test/description` — Test additions/changes
- `chore/description` — Maintenance tasks
