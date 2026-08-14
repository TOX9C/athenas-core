# Migration Guide: Electron → Tauri

This guide covers migrating from the Electron version of Athena's Core to the new Rust/Tauri application.

## Data Compatibility

The Tauri app reads from the **same data directory** as the Electron app. No manual migration is needed for settings, sessions, or tasks.

### Data Directory Locations

| Platform | Path                                          |
| -------- | --------------------------------------------- |
| macOS    | `~/Library/Application Support/athenas-core/` |
| Linux    | `~/.config/athenas-core/`                     |
| Windows  | `%APPDATA%\athenas-core\`                     |

### What Migrates Automatically

| Data Type                           | Format                         | Compatibility                                         |
| ----------------------------------- | ------------------------------ | ----------------------------------------------------- |
| Settings (theme, provider, API key) | `config.json` (electron-store) | ✅ Full — `KeyValueStore` reads the same JSON format  |
| Chat sessions                       | `sessions/` directory          | ✅ Full — `SessionStore` uses the same file structure |
| Kanban tasks                        | `tasks/` in store              | ✅ Full — stored in the same key-value format         |
| Plugin configurations               | `plugins/` in store            | ✅ Full — same plugin manifest format                 |
| Swarm state                         | `.swarm/` directory            | ✅ Full — file-based message passing is unchanged     |

### API Key Migration

The Tauri app will **automatically migrate** your API key to the OS keychain on first launch:

1. On startup, the app checks the OS keychain for an existing key
2. If not found, it falls back to reading from the legacy `config.json`
3. When you save settings, the key is stored in the OS keychain

You can manually verify your key was stored securely:

- **macOS:** Open Keychain Access → search for "athena"
- **Linux:** `secret-tool lookup service athena account api_key`
- **Windows:** Credential Manager → Windows Credentials → search for "athena"

## Feature Parity Matrix

| Feature            | Electron                  | Tauri                      | Notes                                                                                                  |
| ------------------ | ------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------ |
| Terminal (PTY)     | ✅                        | ✅                         | Same `node-pty` → `portable-pty` behavior                                                              |
| Athena AI Chat     | ✅                        | ✅                         | Multi-provider (Anthropic, OpenAI, NVIDIA, LM Studio)                                                  |
| MCP Server         | ✅                        | ✅                         | Port 4545, executor-backed tools plus legacy aliases; clients should use `tools/list` for the live set |
| Agent Comms        | ✅                        | ✅                         | Port 4546, same protocol                                                                               |
| Kanban Board       | ✅                        | ✅                         | Full task management via MCP                                                                           |
| Swarm              | ✅                        | ✅                         | File-based multi-agent coordination                                                                    |
| Plugin System      | ✅                        | ✅                         | Event bus, session management                                                                          |
| Notifications      | ✅                        | ✅                         | Bell, panel, toast                                                                                     |
| Command Palette    | ✅                        | ✅                         | Same shortcuts                                                                                         |
| Settings           | ✅                        | ✅                         | Theme picker, provider config                                                                          |
| File Tree          | ✅                        | ✅                         | With directory listing                                                                                 |
| Workspace Tabs     | ✅                        | ✅                         | Multi-space support                                                                                    |
| Status Bar         | ✅                        | ✅                         | Workspace, pane count, theme                                                                           |
| Keyboard Shortcuts | ✅                        | ⚠️                         | Core shortcuts work; see "Known Differences"                                                           |
| Editor Panel       | ✅ Syntax highlighting    | ⚠️ Read-only text view     | Syntax highlighting coming in Phase 3                                                                  |
| Browser Panel      | ✅ Embedded child webview | ✅ Native child WebView    | Uses an HTTP(S)-only Tauri child WebView; toolbar and native page state are synchronized               |
| Window Controls    | ✅                        | ✅                         | macOS traffic lights, Windows controls                                                                 |
| Auto-update        | ✅                        | ⚠️ Deferred                | No in-app updater is shipped; use the signed DMG/manual update runbook                                 |
| Right Sidebar      | ✅                        | ✅                         | Details, Browser, Output, Assistant tabs                                                               |
| Sidebar Sections   | ✅                        | ✅                         | Spaces, Files, Agents, Plugins                                                                         |
| Theme System       | ✅ 7+ themes              | ⚠️ Theme definitions exist | CSS variable switching in progress                                                                     |
| Font Selection     | ✅                        | ⚠️                         | Font picker coming in Phase 2                                                                          |

## Known Differences

### Keyboard Shortcuts

| Shortcut    | Electron             | Tauri                | Status           |
| ----------- | -------------------- | -------------------- | ---------------- |
| Cmd+T       | New workspace        | New workspace        | ✅               |
| Cmd+K       | Command palette      | Command palette      | ✅               |
| Cmd+J       | Toggle Athena        | Toggle Athena        | ✅               |
| Cmd+B       | Toggle sidebar       | Toggle sidebar       | ✅               |
| Cmd+Shift+P | Command palette      | Command palette      | ✅               |
| Cmd+Shift+S | Settings             | Settings             | ✅               |
| Cmd+1-4     | Switch panel         | Switch panel         | ✅               |
| Cmd+W       | Close tab            | Close tab            | ⚠️ Implemented   |
| Cmd+P       | Quick open           | Command palette      | ⚠️ Opens palette |
| Cmd+E       | Toggle editor        | Toggle editor        | ⚠️ Implemented   |
| Cmd+\       | Toggle right sidebar | Toggle right sidebar | ⚠️ Implemented   |
| Cmd+Shift+R | Reset layout         | Reset layout         | ⚠️ Implemented   |
| Cmd+,       | Settings             | Settings             | ⚠️ Implemented   |
| Escape      | Close modals         | Close modals         | ✅               |
| Cmd+1-9     | Switch workspace     | Switch workspace 1-4 | ⚠️ Only 1-4      |

### Performance Differences

| Metric         | Electron | Tauri     | Improvement |
| -------------- | -------- | --------- | ----------- |
| Binary size    | ~150MB   | ~15MB     | 10x smaller |
| Memory at idle | ~400MB   | ~200MB    | 2x less     |
| Startup time   | ~3s      | ~1s       | 3x faster   |
| PTY throughput | Good     | Excellent | Native Rust |

### UI Differences

- **Title bar:** Electron uses a custom drag region; Tauri uses native title bar on macOS and custom on Windows/Linux
- **Window controls:** macOS shows native traffic lights; Windows/Linux show custom controls
- **Dialogs:** File dialogs use native OS dialogs via Tauri's dialog plugin

## Troubleshooting

### App Won't Start

**Symptom:** The app crashes immediately on launch.

**Solutions:**

1. Check that Rust runtime dependencies are installed (WebView2 on Windows, WebKitGTK on Linux)
2. Delete the config file and restart — corrupted settings can cause crashes:
   ```bash
   # macOS
   rm ~/Library/Application\ Support/athenas-core/config.json
   # Linux
   rm ~/.config/athenas-core/config.json
   ```

### Terminal Not Working

**Symptom:** Terminal panes show but no shell prompt appears.

**Solutions:**

1. Check your default shell is installed: `echo $SHELL`
2. Try setting an explicit shell in Settings
3. Check terminal logs: `log::info!` output goes to the system console

### AI Chat Not Responding

**Symptom:** Sending a message shows no response or an error.

**Solutions:**

1. Verify your API key is set in Settings
2. Check the provider is correctly selected
3. For LM Studio: ensure LM Studio is running on the configured URL
4. Check the system console for API error messages

### MCP Server Connection Fails

**Symptom:** Agents cannot connect to the MCP server.

**Solutions:**

1. Verify port 4545 is not in use: `lsof -i :4545`
2. Check the MCP token matches between the app and agent
3. Ensure the agent is connecting to `127.0.0.1:4545` (not `localhost` which may resolve to IPv6)

### Settings Not Persisting

**Symptom:** Settings reset after closing and reopening the app.

**Solutions:**

1. Check write permissions on the data directory
2. Verify the `KeyValueStore` is writing to the correct path
3. Check for errors in the system console

### Plugin Not Loading

**Symptom:** A plugin that worked in Electron doesn't appear.

**Solutions:**

1. Plugin manifests must be valid JSON — check for syntax errors
2. Plugin directories are scanned at startup — restart the app after adding plugins
3. Check plugin logs in the system console

### Data Directory Not Found

**Symptom:** The app doesn't see your existing settings/sessions.

**Solutions:**

1. Verify the data directory path matches the Electron app's path
2. Tauri uses the same `app.getPath('userData')` equivalent as Electron
3. If you used a custom data directory in Electron, set the `ATHENA_DATA_DIR` environment variable

## Rollback

If you need to return to the Electron version:

1. The Tauri app does **not** modify Electron's data files
2. Both apps can coexist — they read from the same directory
3. Simply launch the Electron app — all your data will be intact

## Support

For issues not covered here:

1. Check the [GitHub issues](https://github.com/your-org/athenas-core/issues) for known problems
2. Include your platform, app version, and relevant logs when reporting bugs
3. Logs can be found in the system console when running `cargo tauri dev`
