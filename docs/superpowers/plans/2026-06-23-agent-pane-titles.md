# Agent Pane Titles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge the two overlapping pane-title settings into one `smart_pane_titles` toggle, fix the "whole prompt flashes as the title" bug, fix Codex never being summarized, fix the count-padding wording, and make title behavior consistent per pane type — with the title lifecycle owned by the backend (retry-with-backoff, empty-while-pending).

**Architecture:** A backend command (`summarize_agent_title`) owns the full title lifecycle: it retries transient LLM failures with backoff and returns a final result (title, sensitive-marker, or a non-retryable failure sentinel). The frontend keeps a small per-pane `TitleState` machine (`Idle | Pending | Failed | Done`) and renders an **empty** pill while pending/failed — the raw prompt is never used as a label, which removes the bug at its root. The two old settings keys migrate to one new key on startup. Codex scraping is extended to read `~/.codex/history.jsonl` so Codex panes finally get summarized.

**Tech Stack:** Rust (Tauri 2 backend + athena-core crate), Dioxus 0.7 WASM frontend, reqwest HTTP client, thiserror for errors, parking_lot for locks. Tests: `#[cfg(test)]` host-native `cargo test` in both frontend and backend crates; `wiremock` for HTTP mocking in the backend.

## Global Constraints

Copied verbatim from the spec (`docs/superpowers/specs/2026-06-23-agent-pane-titles-design.md`):

- One new settings key: `smart_pane_titles` (default `true`). The old keys `auto_generate_titles` and `summarize_agent_titles` are read once at startup for migration and then ignored.
- Migration precedence: `summarize_agent_titles == true` → `true`; else `auto_generate_titles == true` → `true`; else `false`. Idempotent; old keys left in place.
- LLM title style: short sentence, sentence case, 1–6 words, `-ing`/imperative form. No quotes, no trailing punctuation, no preamble.
- `max_tokens` = 48 (raised from 20).
- Sensitive-prompt filter runs before the LLM call and returns `Ok("Sensitive prompt")` immediately — the one non-empty failure path.
- Retry policy: up to 3 attempts, backoff 1s → 2s → 4s (~7s ceiling), then `Err` → frontend `Failed` → empty pill. Missing API key is non-retryable → empty pill (replaces the old `Ok("Untitled")`).
- Per-pane title state: `Idle | Pending | Failed | Done(String)`. `Pending` and `Failed` render an **empty** pill. `Idle` renders the static agent label. `Done` renders the title.
- Idle Shell panes keep `name_for_pane`, gated on `smart_pane_titles`.
- The raw prompt / `task_title` is **never** rendered as a pane label.
- Codex scraping must read `~/.codex/history.jsonl` (`session_id`, `text`, `ts` fields); the old `~/.codex/session_index.jsonl` `thread_name` path is dropped.
- Rust style: `cargo fmt` before commit; `cargo clippy -- -D warnings` clean; no `unwrap()`/`expect()` outside tests and truly-unreachable code; `&str`/`&[T]` over owned params; errors via `Result`/`?`.
- Run the full workspace build before each commit where the change touches Rust: `cargo check --workspace` (fast) or `cargo build --workspace`.

---

## File Structure

**Backend (`src-tauri/` + `crates/`):**

- `crates/athena-core/src/orchestrator.rs` — refactor `summarize_title` to use an injectable base URL + retry-with-backoff. (Existing file; ~1500+ lines, but we touch only the `summarize_title` method and constructor.)
- `crates/athena-core/Cargo.toml` — add `wiremock` dev-dependency.
- `src-tauri/src/commands/mod.rs` — (1) extend `scrape_codex_task` to return a struct + read `history.jsonl`; (2) update the caller to populate Codex `session_id`/`raw_prompt`; (3) update `summarize_agent_title` to map the new backend result (sensitive → `Ok("Sensitive prompt")`, missing key → `Err` sentinel); add scraper + filter tests.
- `src-tauri/src/state.rs` — no change (no new AppState fields; the orchestrator gains the base_url internally).

**Frontend (`frontend/`):**

- `frontend/src/stores/ui.rs` — replace the two `bool` fields with one `smart_pane_titles: bool`; update `Default`.
- `frontend/src/lib.rs` — replace the two `store_get` loads with one migration read.
- `frontend/src/stores/terminal.rs` — replace `summarized_title: Option<String>` with `title_state: TitleState`; add `TitleState` enum; update `TerminalSession::new`, `update_agent_info`.
- `frontend/src/components/workspace/agent_info_poller.rs` — rewrite the summarization trigger to transition the state machine and drop the `summarized_sessions` raw-prompt leak.
- `frontend/src/components/workspace/terminal_grid.rs` — collapse the `left_label` priority ladder to the 4-tier match; extract a pure `resolve_pane_label` fn; add truncation/tooltip in the render; add the ladder truth-table tests.
- `frontend/src/components/settings/settings_modal.rs` — collapse the two toggles into one `Smart pane titles` toggle.

**New pure-function module (extracted for testability, per spec §7):**

- `frontend/src/utils/pane_label.rs` (NEW) — holds `TitleState` and `resolve_pane_label(...)`, the pure fn the ladder and the tests both use. Extracted so the label logic is testable without rendering Dioxus components and so `terminal_grid.rs` stays focused.

**Spec doc:** `docs/superpowers/specs/2026-06-23-agent-pane-titles-design.md` (already committed).

---

## Task 1: Add `wiremock` dev-dependency + injectable Anthropic base URL

**Why first:** The orchestrator's `summarize_title` hardcodes `"https://api.anthropic.com/v1/messages"` with a plain `reqwest::Client`. It is not mockable as-is, so the retry tests (Task 3) cannot be written until the base URL is injectable. This is the minimal refactor that makes the rest testable. Pure infra, no behavior change.

**Files:**
- Modify: `crates/athena-core/Cargo.toml` (add dev-dependency)
- Modify: `crates/athena-core/src/orchestrator.rs:377-435` (struct field + constructor) and `orchestrator.rs:910-960` (summarize_title Anthropic path)

**Interfaces:**
- Consumes: nothing (foundational).
- Produces: a new private field `anthropic_base_url: String` on `AthenaOrchestrator`, defaulting to `"https://api.anthropic.com/v1"`. A constructor `new_for_test(base_url: String)` that tests (Task 3) use to point at a wiremock server. The production constructors (`new`, `with_context`, `new_with_executor`) keep the real default. `summarize_title`'s Anthropic branch builds the URL from this field instead of a literal.

- [ ] **Step 1: Add the dev-dependency**

In `crates/athena-core/Cargo.toml`, find the `[dev-dependencies]` section (currently contains `tempfile = "3"` and `tokio-test = "0.4"`) and add `wiremock`:

```toml
[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
wiremock = "0.6"
```

- [ ] **Step 2: Add the field to the struct**

In `crates/athena-core/src/orchestrator.rs`, in the `AthenaOrchestrator` struct definition (lines 377–402), add a field after `http_client`:

```rust
pub struct AthenaOrchestrator {
    anthropic_messages: Arc<parking_lot::Mutex<Vec<AnthropicMessage>>>,
    openai_messages: Arc<parking_lot::Mutex<Vec<OpenAIMessage>>>,
    current_session_id: Arc<parking_lot::Mutex<Option<String>>>,
    tool_executor: Option<Arc<parking_lot::Mutex<ToolExecutor>>>,
    http_client: reqwest::Client,
    /// Base URL for the Anthropic Messages API. Overridable in tests so the
    /// LLM calls can be mocked with wiremock.
    anthropic_base_url: String,
    provider_config: Arc<parking_lot::Mutex<Option<ProviderConfig>>>,
    rate_limiter: RateLimiter,
    output_buffer: Option<Arc<crate::output_buffer::OutputBuffer>>,
    plan_manager: Option<Arc<crate::plan_manager::PlanManager>>,
    agent_comms: Option<Arc<crate::agent_comms::AgentComms>>,
    workspace_name: Arc<parking_lot::Mutex<Option<String>>>,
    session_store: Option<Arc<athena_store::SessionStore>>,
    kv_store: Option<Arc<athena_store::KeyValueStore>>,
    snapshot_cache: parking_lot::Mutex<Option<(String, Instant)>>,
}
```

- [ ] **Step 3: Initialize the field in all three constructors**

In each of `new()` (line 415), `with_context(...)` (line 439), and `new_with_executor(...)` (line 473), add the field to the struct literal with the real default. For `new()`:

```rust
pub fn new() -> Self {
    Self {
        anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
        openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
        current_session_id: Arc::new(parking_lot::Mutex::new(None)),
        tool_executor: None,
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client"),
        anthropic_base_url: "https://api.anthropic.com/v1".to_string(),
        provider_config: Arc::new(parking_lot::Mutex::new(None)),
        rate_limiter: RateLimiter::new(1000),
        output_buffer: None,
        plan_manager: None,
        agent_comms: None,
        workspace_name: Arc::new(parking_lot::Mutex::new(None)),
        session_store: None,
        kv_store: None,
        snapshot_cache: parking_lot::Mutex::new(None),
    }
}
```

Apply the identical `anthropic_base_url: "https://api.anthropic.com/v1".to_string(),` line to the other two constructors in the same position (after `http_client`).

- [ ] **Step 4: Add the test constructor**

Add immediately after `new()` (before `with_context`):

```rust
    /// Test-only constructor that points the Anthropic base URL at a mock
    /// server (e.g. wiremock). Not compiled into release paths that matter,
    /// but available to `#[cfg(test)]` callers.
    #[cfg(test)]
    pub fn new_for_test(anthropic_base_url: String) -> Self {
        let mut orch = Self::new();
        orch.anthropic_base_url = anthropic_base_url;
        orch
    }
```

- [ ] **Step 5: Use the field in `summarize_title`'s Anthropic branch**

In `summarize_title` (line ~936), replace the hardcoded URL:

```rust
                let response = client
                    .post(format!("{}/messages", self.anthropic_base_url))
```

(replacing `client.post("https://api.anthropic.com/v1/messages")`). Leave the rest of the Anthropic branch untouched.

- [ ] **Step 6: Build and run existing tests to confirm no behavior change**

Run: `cargo check --workspace 2>&1 | tail -20`
Expected: compiles clean (warnings about the unused `new_for_test`/`anthropic_base_url` outside tests are fine for now; Task 3 exercises them).

Run: `cargo test -p athena-core 2>&1 | tail -20`
Expected: all existing tests still PASS (no behavior change — production constructors keep the real URL).

- [ ] **Step 7: Commit**

```bash
git add crates/athena-core/Cargo.toml crates/athena-core/src/orchestrator.rs
git commit -m "refactor: make Anthropic base URL injectable for title tests

Add anthropic_base_url field + new_for_test constructor so the LLM
title summarizer can be pointed at a wiremock server. Production
constructors keep the real api.anthropic.com default; no behavior change."
```

---

## Task 2: Rewrite `summarize_title` prompt + retry-with-backoff

**Why:** This implements spec §2 (new wording, `max_tokens` 48) and §3 (backend retry loop). Depends on Task 1 (injectable base URL) so the retry tests in Task 3 can mock 5xx responses.

**Files:**
- Modify: `crates/athena-core/src/orchestrator.rs:908-999` (the whole `summarize_title` method)

**Interfaces:**
- Consumes: `anthropic_base_url` and `new_for_test` from Task 1; `OrchestratorError::{HttpError, MissingApiKey, Generic}` from `crates/athena-core/src/types.rs`.
- Produces: `summarize_title(&self, raw_prompt: &str) -> Result<String, OrchestratorError>` with the new contract: returns `Ok(title)` on success (after possible retries); returns `Err(MissingApiKey)` when no provider/env key (non-retryable); returns `Err(Generic(...))` after exhausting retries on retryable HTTP/parse errors.

- [ ] **Step 1: Write the failing test for retry-then-success**

Add a new test module (or append to the existing one near line 1451) in `crates/athena-core/src/orchestrator.rs`. These tests mock the Anthropic endpoint with wiremock and verify the retry loop. **For test speed, override the backoff to near-zero** by exposing a test seam (see Step 3). First, the retry-then-success test:

```rust
#[cfg(test)]
mod title_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // A tiny provider config so summarize_title finds a key without env vars.
    fn test_provider_config(mock_server_url: String) -> ProviderConfig {
        ProviderConfig::new(
            LLMProvider::Anthropic,
            secrecy::SecretString::from("test-key".to_string()),
            "claude-3-5-sonnet-20241022".to_string(),
            String::new(),
            Some(mock_server_url),
        )
    }

    #[tokio::test]
    async fn summarize_title_retries_on_5xx_then_succeeds() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        // First two calls fail 500, third succeeds.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "analyzing the codebase" }]
            })))
            .mount(&server)
            .await;

        let title = orch.summarize_title_for_test("analyze the codebase").await.unwrap();
        assert_eq!(title, "analyzing the codebase");
    }
}
```

> Note: the test calls `summarize_title_for_test` — a thin wrapper we add in Step 3 that runs the real `summarize_title` logic but with a near-zero backoff, so the test doesn't sleep 1s+2s. This keeps the production backoff (1s/2s/4s) intact while making tests fast. If the codebase's `ProviderConfig::new` signature differs, adjust the constructor args to match the real one (the Explore agent confirmed fields `provider, api_key, model, system_prompt, base_url`).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p athena-core title_tests::summarize_title_retries_on_5xx_then_succeeds 2>&1 | tail -30`
Expected: FAIL — `summarize_title_for_test` does not exist (compile error). This is the expected RED state.

- [ ] **Step 3: Rewrite `summarize_title` with the new prompt + retry loop + test seam**

Replace the entire `summarize_title` method (lines 908–999) with:

```rust
    /// Send a one-shot request to the configured LLM to summarize a prompt
    /// into a short title. Does NOT touch conversation history.
    ///
    /// Retries transient failures (network / 5xx / parse) with backoff up to
    /// `MAX_ATTEMPTS` times, then returns `Err`. A missing API key is
    /// non-retryable (returns `Err(MissingApiKey)` immediately).
    pub async fn summarize_title(&self, raw_prompt: &str) -> Result<String, OrchestratorError> {
        self.summarize_title_with_backoff(raw_prompt, DEFAULT_BACKOFF_DELAYS_MS).await
    }

    /// Test seam: same as `summarize_title` but with a (near-zero) backoff
    /// schedule so retry tests don't sleep for seconds.
    #[cfg(test)]
    pub async fn summarize_title_for_test(&self, raw_prompt: &str) -> Result<String, OrchestratorError> {
        self.summarize_title_with_backoff(raw_prompt, &[1u64, 1, 1]).await
    }

    async fn summarize_title_with_backoff(
        &self,
        raw_prompt: &str,
        backoff_delays_ms: &[u64],
    ) -> Result<String, OrchestratorError> {
        const SYSTEM: &str = "You write short, descriptive titles for coding sessions based on the user's first prompt.\nWrite a short sentence in sentence case describing what the agent is doing, 1-6 words.\nUse the imperative or -ing form (e.g. \"analyzing the codebase\", \"checking rust version\", \"fixing the login bug\").\nNo quotes, no trailing punctuation, no preamble - output only the title.";
        let prompt = format!("{}", raw_prompt);

        let config = { self.provider_config.lock().as_ref().cloned() };

        let (provider, api_key, model, base_url) = match config {
            Some(c) => (c.provider.clone(), c.api_key().clone(), c.model.clone(), c.base_url.clone()),
            None => {
                let api_key = std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .ok_or(OrchestratorError::MissingApiKey)?;
                (LLMProvider::Anthropic, secrecy::SecretString::from(api_key), DEFAULT_ANTHROPIC_MODEL.to_string(), None)
            }
        };

        let max_attempts = backoff_delays_ms.len();
        let client = &self.http_client;

        let mut last_err: Option<OrchestratorError> = None;
        for attempt in 0..max_attempts {
            match self.one_title_call(client, provider.clone(), api_key.clone(), model.clone(), base_url.clone(), SYSTEM, &prompt).await {
                Ok(title) => return Ok(title),
                Err(e) => {
                    // MissingApiKey is non-retryable; propagate immediately.
                    if matches!(e, OrchestratorError::MissingApiKey) {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
            // Back off before the next attempt (skip after the last attempt).
            if attempt + 1 < max_attempts {
                tokio::time::sleep(std::time::Duration::from_millis(backoff_delays_ms[attempt])).await;
            }
        }
        Err(last_err.unwrap_or_else(|| OrchestratorError::Generic("title generation failed with no attempts".to_string())))
    }

    /// Single LLM call for title summarization. Returns the parsed title on
    /// success, or an error the caller decides whether to retry.
    async fn one_title_call(
        &self,
        client: &reqwest::Client,
        provider: LLMProvider,
        api_key: secrecy::SecretString,
        model: String,
        base_url: Option<String>,
        system: &str,
        prompt: &str,
    ) -> Result<String, OrchestratorError> {
        match provider {
            LLMProvider::Anthropic => {
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 48,
                    "system": system,
                    "messages": [{"role": "user", "content": prompt}]
                });
                let response = client
                    .post(format!("{}/messages", self.anthropic_base_url))
                    .header("x-api-key", api_key.expose_secret())
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!("Anthropic API error {}: {}", status, err_text)));
                }
                let json: serde_json::Value = response.json().await?;
                let content = json["content"].as_array().ok_or_else(|| OrchestratorError::Generic("Invalid Anthropic response: no content array".to_string()))?;
                let summary = content.iter()
                    .filter(|b| b["type"].as_str() == Some("text"))
                    .map(|b| b["text"].as_str().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                Ok(summary)
            }
            LLMProvider::OpenAI | LLMProvider::NvidiaNim | LLMProvider::Lmstudio => {
                let url = match &provider {
                    LLMProvider::NvidiaNim => "https://integrate.api.nvidia.com/v1".to_string(),
                    LLMProvider::OpenAI => "https://api.openai.com/v1".to_string(),
                    LLMProvider::Lmstudio => base_url.unwrap_or_else(|| "http://localhost:1234/v1".to_string()),
                    _ => unreachable!(),
                };
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 48,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": prompt}
                    ],
                    "stream": false,
                });
                let response = client
                    .post(format!("{}/chat/completions", url))
                    .header("Authorization", format!("Bearer {}", api_key.expose_secret()))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!("OpenAI API error {}: {}", status, err_text)));
                }
                let json: serde_json::Value = response.json().await?;
                let summary = json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("Summary")
                    .trim()
                    .to_string();
                Ok(summary)
            }
        }
    }
```

Add the constants near the top of the file (with the other module-level constants, e.g. near `DEFAULT_ANTHROPIC_MODEL`):

```rust
/// Backoff schedule (ms) between title-summarization retries. ~7s ceiling.
const DEFAULT_BACKOFF_DELAYS_MS: &[u64] = &[1000, 2000, 4000];
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p athena-core title_tests::summarize_title_retries_on_5xx_then_succeeds 2>&1 | tail -30`
Expected: PASS.

- [ ] **Step 5: Add the remaining backend tests (fail-after-max, missing-key non-retryable, trim)**

Append to the `title_tests` module:

```rust
    #[tokio::test]
    async fn summarize_title_fails_after_max_attempts() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        // Every call fails 500.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let result = orch.summarize_title_for_test("analyze the codebase").await;
        assert!(result.is_err());
        // 3 attempts made (test backoff = [1,1,1]).
        // wiremock records request count:
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 3, "expected exactly 3 attempts, got {}", received.len());
    }

    #[tokio::test]
    async fn summarize_title_missing_key_is_non_retryable() {
        // No provider config AND no ANTHROPIC_API_KEY env (tests run without it).
        // Temporarily ensure the env var is unset for this test's scope.
        let orch = AthenaOrchestrator::new_for_test("http://unused.invalid".to_string());
        // provider_config stays None.
        // SAFETY: tests run single-threaded within a process; we save & restore.
        let prev = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");
        let result = orch.summarize_title_for_test("analyze the codebase").await;
        // Restore.
        if let Some(v) = prev { std::env::set_var("ANTHROPIC_API_KEY", v); }

        assert!(matches!(result, Err(OrchestratorError::MissingApiKey)));
    }

    #[tokio::test]
    async fn summarize_title_trims_output() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "  analyzing the codebase  " }]
            })))
            .mount(&server)
            .await;

        let title = orch.summarize_title_for_test("analyze the codebase").await.unwrap();
        assert_eq!(title, "analyzing the codebase");
    }
```

- [ ] **Step 6: Run all backend title tests**

Run: `cargo test -p athena-core title_tests 2>&1 | tail -30`
Expected: all 4 tests PASS.

- [ ] **Step 7: Run clippy + fmt**

Run: `cargo clippy -p athena-core -- -D warnings 2>&1 | tail -20`
Expected: no warnings (fix any that appear — common ones: needless clone of `provider`/`api_key`; if clippy objects to cloning the `SecretString`, clone via `.clone()` only where the borrow checker requires it across the loop).

Run: `cargo fmt`
Expected: reformats; no functional change.

- [ ] **Step 8: Commit**

```bash
git add crates/athena-core/src/orchestrator.rs
git commit -m "feat: LLM title summarizer with retry-with-backoff + new wording

summarize_title now retries transient failures (network/5xx/parse) up
to 3x with 1s/2s/4s backoff, returns Err(MissingApiKey) immediately
(non-retryable), and uses a -ing/imperative sentence-case prompt with
max_tokens 48. Adds wiremock-backed tests."
```

---

## Task 3: Update `summarize_agent_title` command (sensitive filter + missing-key sentinel)

**Why:** The Tauri command is the bridge between the frontend and `orchestrator.summarize_title`. Spec §3 requires: sensitive prompt → `Ok("Sensitive prompt")` (non-retryable, no LLM call); missing key → a sentinel the frontend maps to `Failed` (replaces the old `Ok("Untitled")`). This task keeps the existing sensitive filter and re-maps the missing-key path. Depends on Task 2's new `summarize_title` contract.

**Files:**
- Modify: `src-tauri/src/commands/mod.rs:2137-2188` (`summarize_agent_title` command)

**Interfaces:**
- Consumes: `orchestrator.summarize_title` (Task 2) returning `Result<String, OrchestratorError>` with `MissingApiKey` non-retryable.
- Produces: the command `summarize_agent_title(raw_prompt: String) -> Result<String, String>` with the new contract:
  - `Ok("Sensitive prompt")` — sensitive prompt detected (unchanged).
  - `Ok(title)` — LLM succeeded.
  - `Err(...)` — any other failure (missing key, retries exhausted). The frontend (Task 6) treats `Err` as `Failed` → empty pill.

- [ ] **Step 1: Write the failing test for sensitive-prompt blocking**

The sensitive-filter logic is currently inline in the command. To make it testable, extract it into a pure function `prompt_is_sensitive(raw_prompt: &str) -> bool` in the same file. Write the test first in a new `#[cfg(test)] mod tests` at the bottom of `src-tauri/src/commands/mod.rs` (if one already exists, append):

```rust
#[cfg(test)]
mod title_command_tests {
    use super::prompt_is_sensitive;

    #[test]
    fn sensitive_prompt_blocks_plaintext_variants() {
        for kw in ["my password is x", "set the API_KEY=..", "a secret token", "auth header here", "credential leak"] {
            assert!(prompt_is_sensitive(kw), "expected sensitive: {kw}");
        }
    }

    #[test]
    fn sensitive_prompt_blocks_l33t_variants() {
        for kw in ["p@ssword", "t0k3n", "API_K3Y", "s3cret"] {
            assert!(prompt_is_sensitive(kw), "expected l33t-sensitive: {kw}");
        }
    }

    #[test]
    fn normal_prompt_passes_filter() {
        assert!(!prompt_is_sensitive("analyze the codebase"));
        assert!(!prompt_is_sensitive("what rust version is this"));
        assert!(!prompt_is_sensitive("hi"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p athena-core --lib 2>&1 | tail -5` (warm check) then `cargo test --manifest-path src-tauri/Cargo.toml title_command_tests 2>&1 | tail -30`
Expected: FAIL — `prompt_is_sensitive` not defined.

- [ ] **Step 3: Extract `prompt_is_sensitive` and rewrite the command**

Add the pure function near the top of `src-tauri/src/commands/mod.rs` (e.g. just before the `summarize_agent_title` command at line ~2137):

```rust
/// Returns true if `raw_prompt` looks like it contains a secret we must not
/// send to the LLM. Checks plaintext keywords and common l33t-sp34k
/// substitutions (a=@, o=0, e=3, i=1/!, s=$) so trivial obfuscation does not
/// bypass it.
fn prompt_is_sensitive(raw_prompt: &str) -> bool {
    let lowercase = raw_prompt.to_lowercase();
    let plaintext = [
        "password", "passw0rd", "p@ssword",
        "token", "t0ken", "t0k3n",
        "secret", "s3cret", "s3cr3t",
        "api_key", "apikey", "api-key", "api_k3y",
        "authorization", "auth", "4uth",
        "credential", "cr3dential",
        "private key", "passphrase", "pin",
    ];
    if plaintext.iter().any(|&kw| lowercase.contains(kw)) {
        return true;
    }
    let normalized = lowercase
        .replace('@', "a")
        .replace('0', "o")
        .replace('3', "e")
        .replace('1', "i")
        .replace('!', "i")
        .replace('$', "s");
    let normalized_keywords = [
        "password", "token", "secret", "api_key", "apikey", "api-key",
        "authorization", "auth", "credential", "private key", "passphrase", "pin",
    ];
    normalized_keywords.iter().any(|&kw| normalized.contains(kw))
}
```

Now rewrite the `summarize_agent_title` command (lines 2137–2188) to use it and re-map the missing-key path:

```rust
/// Summarize a prompt into a short title using the configured LLM.
/// Does NOT touch conversation history.
///
/// Contract:
/// - `Ok("Sensitive prompt")` — prompt matched the sensitive filter (no LLM call).
/// - `Ok(title)` — the LLM produced a title.
/// - `Err(_)` — missing API key OR retries exhausted. The frontend maps this
///   to a `Failed` title state (empty pill).
#[tauri::command]
pub async fn summarize_agent_title(state: State<'_, AppState>, raw_prompt: String) -> Result<String, String> {
    // Block obviously sensitive prompts before sending anything.
    if prompt_is_sensitive(&raw_prompt) {
        return Ok("Sensitive prompt".to_string());
    }

    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            // Non-retryable: surface an error so the frontend shows an empty
            // pill (Failed) instead of a misleading "Untitled".
            return Err("no api key configured".to_string());
        }
    }
    orchestrator
        .summarize_title(&raw_prompt)
        .await
        .map(|t| t.trim().to_string())
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml title_command_tests 2>&1 | tail -20`
Expected: all 3 filter tests PASS.

- [ ] **Step 5: Build the whole app to confirm the command still registers**

Run: `cargo check --workspace 2>&1 | tail -20`
Expected: compiles clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/mod.rs
git commit -m "refactor: extract sensitive-prompt filter; map missing-key to Err

summarize_agent_title now returns Ok(\"Sensitive prompt\") for blocked
prompts (no LLM call) and Err for missing API key (replacing the old
Ok(\"Untitled\")), so the frontend can map missing-key to an empty
Failed pill. Extracts prompt_is_sensitive as a pure tested fn."
```

---

## Task 4: Extend Codex scraper + update caller

**Why:** Spec §4. Today Codex never returns `session_id`/`raw_prompt` (reads the wrong file — `session_index.jsonl` `thread_name`), so Codex panes never get summarized. Extend `scrape_codex_task` to read `~/.codex/history.jsonl` and return a struct mirroring `ClaudeHistoryEntry`; update the caller to populate all four fields for Codex. This is the core of "make it agent-specific."

**Files:**
- Modify: `src-tauri/src/commands/mod.rs:1668-1724` (the `AgentInfo`, `ClaudeHistoryEntry`, `scrape_claude_task`, `scrape_codex_task` block) and `1833-1845` (the caller match).

**Interfaces:**
- Consumes: nothing new.
- Produces: a new `CodexHistoryEntry { display, session_id, timestamp }` struct and a rewritten `scrape_codex_task() -> Option<CodexHistoryEntry>`. The `pty_agent_info` caller now returns `(Some(display), Some(session_id), Some(timestamp), Some(raw_prompt))` for the `"codex"` arm (mirroring Claude).

- [ ] **Step 1: Write the failing tests for the Codex scraper**

The scrapers read from `$HOME`. To make them testable without clobbering the user's real files, **refactor both scrapers to take a `&Path`-style home dir** — i.e. extract the parsing into `parse_codex_history(content: &str) -> Option<CodexHistoryEntry>` and `parse_claude_history(content: &str) -> Option<ClaudeHistoryEntry>`, and have the file-reading wrappers call them. Write the tests against the pure parsers.

Add (extending the `title_command_tests` module from Task 3, or a new module):

```rust
#[cfg(test)]
mod scraper_tests {
    use super::{parse_claude_history, parse_codex_history};

    #[test]
    fn parse_codex_history_extracts_session_and_prompt() {
        let line = r#"{"session_id":"019e0ec8-7793-77a3-b52c-c153ed517b64","ts":1778364486,"text":"yo what is up"}"#;
        let entry = parse_codex_history(line).expect("should parse");
        assert_eq!(entry.display, "yo what is up");
        assert_eq!(entry.session_id, "019e0ec8-7793-77a3-b52c-c153ed517b64");
        assert_eq!(entry.timestamp, 1778364486);
    }

    #[test]
    fn parse_codex_history_returns_none_on_malformed() {
        assert!(parse_codex_history("not json").is_none());
        assert!(parse_codex_history(r#"{"no_text":"here"}"#).is_none());
    }

    #[test]
    fn parse_claude_history_parses_display_session_timestamp() {
        let line = r#"{"display":"analyze the codebase","sessionId":"abc-123","timestamp":1700000000000}"#;
        let entry = parse_claude_history(line).expect("should parse");
        assert_eq!(entry.display, "analyze the codebase");
        assert_eq!(entry.session_id, "abc-123");
        assert_eq!(entry.timestamp, 1700000000000);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml scraper_tests 2>&1 | tail -20`
Expected: FAIL — `parse_codex_history` / `parse_claude_history` not defined.

- [ ] **Step 3: Add the `CodexHistoryEntry` struct and pure parsers**

Replace the block at lines 1683–1724 with:

```rust
/// Metadata scraped from the last entry in Claude's history file.
#[derive(Debug, Clone)]
struct ClaudeHistoryEntry {
    /// The raw prompt text the user typed.
    display: String,
    /// The session UUID this prompt belongs to.
    session_id: String,
    /// Unix timestamp (ms) when the prompt was sent.
    timestamp: u64,
}

/// Metadata scraped from the last entry in Codex's history file.
/// Mirrors `ClaudeHistoryEntry` so both agents feed the title pipeline
/// uniformly. (Previously Codex read `session_index.jsonl`'s `thread_name`,
/// which is a thread name — not a prompt — and carried no session id, so
/// Codex panes were never summarized.)
#[derive(Debug, Clone)]
struct CodexHistoryEntry {
    /// The raw prompt text the user typed (`text` field).
    display: String,
    /// The session id (`session_id` field).
    session_id: String,
    /// Unix timestamp (s) from the `ts` field.
    timestamp: u64,
}

/// Parse the last line of Claude's `~/.claude/history.jsonl` into an entry.
/// Pure (no I/O) so it is unit-testable.
fn parse_claude_history(content: &str) -> Option<ClaudeHistoryEntry> {
    let last_line = content.lines().last()?;
    let json: serde_json::Value = serde_json::from_str(last_line).ok()?;
    let display = json.get("display")?.as_str()?.trim().to_string();
    let session_id = json.get("sessionId")?.as_str()?.to_string();
    let timestamp = json.get("timestamp")?.as_u64()?;
    Some(ClaudeHistoryEntry { display, session_id, timestamp })
}

/// Parse the last line of Codex's `~/.codex/history.jsonl` into an entry.
/// Pure (no I/O) so it is unit-testable.
fn parse_codex_history(content: &str) -> Option<CodexHistoryEntry> {
    let last_line = content.lines().last()?;
    let json: serde_json::Value = serde_json::from_str(last_line).ok()?;
    let display = json.get("text")?.as_str()?.trim().to_string();
    let session_id = json.get("session_id")?.as_str()?.to_string();
    let timestamp = json.get("ts")?.as_u64()?;
    Some(CodexHistoryEntry { display, session_id, timestamp })
}

/// Scrape the last session entry from Claude's history file.
fn scrape_claude_task() -> Option<ClaudeHistoryEntry> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".claude/history.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    parse_claude_history(&content)
}

/// Scrape the last session entry from Codex's history file.
fn scrape_codex_task() -> Option<CodexHistoryEntry> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".codex/history.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    parse_codex_history(&content)
}
```

- [ ] **Step 4: Update the caller in `pty_agent_info`**

Replace lines 1833–1845 (the `match process.as_str()` block) so the Codex arm mirrors Claude:

```rust
    let (task_title, session_id, timestamp, raw_prompt) = match process.as_str() {
        "claude" => match scrape_claude_task() {
            Some(entry) => {
                let raw = entry.display.clone();
                (Some(entry.display), Some(entry.session_id), Some(entry.timestamp), Some(raw))
            }
            None => (None, None, None, None),
        },
        "codex" => match scrape_codex_task() {
            Some(entry) => {
                let raw = entry.display.clone();
                (Some(entry.display), Some(entry.session_id), Some(entry.timestamp), Some(raw))
            }
            None => (None, None, None, None),
        },
        _ => (None, None, None, None),
    };
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml scraper_tests 2>&1 | tail -20`
Expected: all 3 scraper tests PASS.

- [ ] **Step 6: Build the whole app**

Run: `cargo check --workspace 2>&1 | tail -20`
Expected: compiles clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/mod.rs
git commit -m "fix: summarize Codex panes (read ~/.codex/history.jsonl)

scrape_codex_task now reads ~/.codex/history.jsonl and returns a
CodexHistoryEntry (display, session_id, timestamp), mirroring Claude.
Previously it read session_index.jsonl's thread_name (a thread name,
not a prompt) and returned no session id, so Codex panes never
entered the summarization path. Extracts parse_{claude,codex}_history
pure parsers with tests."
```

---

## Task 5: Frontend `TitleState` enum + `resolve_pane_label` pure fn (NEW module)

**Why first in the frontend:** This is the pure, tested core the ladder (Task 8) and the store (Task 6) both depend on. Spec §5 + §7. Building it as its own focused file keeps `terminal_grid.rs` lean and makes the label logic testable without rendering.

**Files:**
- Create: `frontend/src/utils/pane_label.rs`
- Modify: `frontend/src/utils/mod.rs` (export the new module)

**Interfaces:**
- Consumes: `AgentType` from `crate::types::workspace`, `name_for_pane` from `crate::utils::pane_names`.
- Produces: `pub enum TitleState { Idle, Pending, Failed, Done(String) }` and `pub fn resolve_pane_label(label: Option<&str>, title_state: &TitleState, agent_type: &AgentType, fg_process: Option<&str>, smart_on: bool, static_agent_label: &str) -> String`. `static_agent_label` is the caller-supplied static label ("Claude Code", etc.) so the pure fn doesn't need UI/i18n access.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/utils/pane_label.rs` with the test module first:

```rust
//! Pure pane-label resolution for the title state machine.
//!
//! Extracted from `terminal_grid.rs` so the priority ladder is unit-testable
//! without rendering a Dioxus component. The raw prompt / scraped `task_title`
//! is NEVER used as a label here — that is the root fix for the
//! "whole prompt flashes as the title" bug. Only `TitleState::Done` (an LLM
//! title) or the static/random fallbacks are ever returned.

use crate::types::workspace::AgentType;
use crate::utils::pane_names::name_for_pane;

/// Per-pane title state, owned by the frontend store. The backend owns the
/// retry loop; the frontend only tracks whether a title is expected, in
/// flight, failed, or available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleState {
    /// No prompt scraped yet.
    Idle,
    /// Prompt scraped; the LLM call (with backend retries) is in flight.
    Pending,
    /// Backend exhausted retries or hit a non-retryable error.
    Failed,
    /// A title (or "Sensitive prompt") was produced.
    Done(String),
}

impl Default for TitleState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Resolve the visible left label for a pane. Priority:
///   1. user rename (`label`)
///   2. idle Shell → random name (only when `smart_on`)
///   3. agent pane → render `TitleState`
///   4. everything else → static agent label
///
/// `Pending` and `Failed` render an empty string (the raw prompt is never
/// shown). `Done` renders the title verbatim (truncation is view-only).
pub fn resolve_pane_label(
    label: Option<&str>,
    title_state: &TitleState,
    agent_type: &AgentType,
    fg_process: Option<&str>,
    smart_on: bool,
    static_agent_label: &str,
) -> String {
    // 1. User rename always wins.
    if let Some(l) = label {
        if !l.is_empty() {
            return l.to_string();
        }
    }

    let is_idle_shell = *agent_type == AgentType::Shell
        && fg_process.map_or(true, |p| p == "shell" || p.is_empty());

    // 2. Idle Shell → random name, only when smart titles are on.
    if is_idle_shell && smart_on {
        return name_for_pane(""); // pane_id not needed for determinism in tests
    }

    // 3. Agent pane → render TitleState.
    match title_state {
        TitleState::Done(title) => title.clone(),
        TitleState::Idle => static_agent_label.to_string(),
        // Pending / Failed → empty pill. The raw prompt is never shown.
        TitleState::Pending | TitleState::Failed => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> AgentType { AgentType::Shell }
    fn claude() -> AgentType { AgentType::Claude }

    #[test]
    fn user_rename_wins_regardless_of_state() {
        for state in [TitleState::Idle, TitleState::Pending, TitleState::Failed, TitleState::Done("x".into())] {
            assert_eq!(
                resolve_pane_label(Some("my pane"), &state, &claude(), None, true, "Claude Code"),
                "my pane"
            );
        }
    }

    #[test]
    fn empty_rename_falls_through() {
        assert_eq!(
            resolve_pane_label(Some(""), &TitleState::Idle, &claude(), None, true, "Claude Code"),
            "Claude Code"
        );
    }

    #[test]
    fn idle_agent_shows_static_label() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Idle, &claude(), None, true, "Claude Code"),
            "Claude Code"
        );
    }

    #[test]
    fn pending_shows_empty() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Pending, &claude(), None, true, "Claude Code"),
            ""
        );
    }

    #[test]
    fn failed_shows_empty() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Failed, &claude(), None, true, "Claude Code"),
            ""
        );
    }

    #[test]
    fn done_shows_title() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Done("analyzing the codebase".into()), &claude(), None, true, "Claude Code"),
            "analyzing the codebase"
        );
    }

    #[test]
    fn done_sensitive_prompt_shows_marker() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Done("Sensitive prompt".into()), &claude(), None, true, "Claude Code"),
            "Sensitive prompt"
        );
    }

    #[test]
    fn idle_shell_smart_on_shows_random_name() {
        let name = resolve_pane_label(None, &TitleState::Idle, &shell(), None, true, "Shell");
        assert!(!name.is_empty());
        assert_ne!(name, "Shell");
    }

    #[test]
    fn idle_shell_smart_off_shows_static_label() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Idle, &shell(), None, false, "Shell"),
            "Shell"
        );
    }

    #[test]
    fn running_non_shell_process_falls_through_to_state() {
        // A shell running 'vim' is not "idle" — fg_process = "vim".
        assert_eq!(
            resolve_pane_label(None, &TitleState::Pending, &shell(), Some("vim"), true, "Shell"),
            ""
        );
        assert_eq!(
            resolve_pane_label(None, &TitleState::Done("editing".into()), &shell(), Some("vim"), true, "Shell"),
            "editing"
        );
    }
}
```

> Note on `name_for_pane("")`: the existing `pane_names` tests confirm an empty id does not panic and returns a non-empty name, so this is safe for the deterministic test. The real call site (Task 8) passes the actual pane id; only the test uses `""`.

- [ ] **Step 2: Run the test to verify it fails (module not wired yet)**

Run: `cargo test --manifest-path frontend/Cargo.toml pane_label 2>&1 | tail -30`
Expected: FAIL — module not declared in `utils/mod.rs` (compile error: unresolved import).

- [ ] **Step 3: Export the module**

In `frontend/src/utils/mod.rs`, add:

```rust
pub mod pane_label;
```

(Add the line among the other `pub mod ...;` declarations, keeping alphabetical/order-with-neighbors.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path frontend/Cargo.toml pane_label 2>&1 | tail -30`
Expected: all tests in `pane_label::tests` PASS.

- [ ] **Step 5: fmt + clippy**

Run: `cd frontend && cargo fmt && cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/utils/pane_label.rs frontend/src/utils/mod.rs
git commit -m "feat: pure TitleState + resolve_pane_label for pane labels

New utils/pane_label module holds the title state machine (Idle /
Pending / Failed / Done) and the resolve_pane_label priority ladder.
Pending/Failed render empty — the raw prompt is never a label, which
is the root fix for the whole-prompt-flashes bug. Extracted as a pure
fn for unit testing."
```

---

## Task 6: Wire `TitleState` into the terminal store

**Why:** Replace the single `summarized_title: Option<String>` field with `title_state: TitleState` so the poller (Task 7) and ladder (Task 8) can use the state machine. Depends on Task 5.

**Files:**
- Modify: `frontend/src/stores/terminal.rs` (struct field + `new` + `update_agent_info`)

**Interfaces:**
- Consumes: `TitleState` from `crate::utils::pane_label`.
- Produces: `TerminalSession.title_state: TitleState` (replacing `summarized_title`). `update_agent_info` keeps its signature (the poller still writes `raw_prompt` so it can drive the state machine in Task 7), but no longer derives a label from `task_title`.

- [ ] **Step 1: Replace the field**

In `frontend/src/stores/terminal.rs`, replace the `summarized_title` field (line 264–265) and its doc comment:

```rust
    /// Raw prompt text (scraped from the agent's history file; used by the
    /// poller to decide when to request a title — never rendered as a label).
    pub raw_prompt: Option<String>,
    /// Per-pane title state machine. Idle until a prompt is scraped, Pending
    /// while the LLM call is in flight, Failed if it exhausted retries, Done
    /// once a title (or "Sensitive prompt") is available. See utils/pane_label.
    pub title_state: crate::utils::pane_label::TitleState,
```

(Remove the old `summarized_title` field entirely.)

- [ ] **Step 2: Update `TerminalSession::new`**

In `new()` (line 270–296), replace `summarized_title: None,` with:

```rust
            title_state: crate::utils::pane_label::TitleState::default(),
```

- [ ] **Step 3: Update `update_agent_info`**

Find `update_agent_info` (the Explore agent placed it at lines 522–543). It currently sets `summarized_title` indirectly or not at all. Ensure it writes `raw_prompt` and leaves `title_state` untouched here (the poller manages the transition in Task 7). Concretely: remove any line that sets `session.summarized_title`, keep the `session.raw_prompt = raw_prompt;` write. If `update_agent_info` does not currently write `raw_prompt`, add:

```rust
session.raw_prompt = raw_prompt;
```

Read the actual function body first and edit minimally — do not rewrite unrelated lines.

- [ ] **Step 4: Build the frontend**

Run: `cd frontend && cargo check 2>&1 | tail -30`
Expected: likely a few compile errors in `agent_info_poller.rs` and `terminal_grid.rs` that still reference `summarized_title` — those are fixed in Tasks 7 and 8. **It is OK for this task to leave the crate non-compiling**; the next two tasks complete the wiring. Do NOT commit yet — this task's commit is folded into Task 8 once the crate compiles again.

> Task-boundary note: this task deliberately does not end green. The store field change ripples into two files that are fixed immediately after. Committing a non-compiling state is worse than batching, so the commit happens at the end of Task 8. If the reviewer gate wants a green commit sooner, an alternative is to keep `summarized_title` as a deprecated alias returning `None` until Task 8 — but the simpler path is the batched commit in Task 8. Proceed to Task 7.

---

## Task 7: Rewrite the poller's summarization trigger (state machine)

**Why:** The poller must transition the state machine `Idle → Pending`, call the backend once (which now retries internally), and set `Done` or `Failed`. Spec §3. It must also add the empty/whitespace-prompt guard. Depends on Tasks 5 and 6.

**Files:**
- Modify: `frontend/src/components/workspace/agent_info_poller.rs:68-117` (the summarization trigger block)

**Interfaces:**
- Consumes: `summarize_agent_title` from `tauri_bridge`, `TitleState` + `TerminalSession.title_state` (Tasks 5–6), `smart_pane_titles` from `ui` store (Task 9 wires the store field; until then the poller reads whatever the field is named — use `smart_pane_titles` and Task 9 makes it exist).
- Produces: the poller drives `TitleState` transitions: `Idle → Pending` on first prompt for a session, then `Done(summary)` or `Failed` on the command result. The `summarized_sessions` HashSet stays as the "already handled this session" guard (now meaning "already entered Pending", not "already summarized").

- [ ] **Step 1: Read the current poller block to edit precisely**

Run: read `frontend/src/components/workspace/agent_info_poller.rs` lines 60–120 (already captured during planning: the block at 68–109).

- [ ] **Step 2: Rewrite the trigger block**

Replace lines 68–109 (the `// Trigger LLM summarization …` block through the end of the `spawn_local`) with:

```rust
                            // Drive the title state machine for a new session.
                            let sid = info.session_id.as_deref().unwrap_or_default();
                            let feature_enabled = ui_state.read().smart_pane_titles;
                            let raw = info.raw_prompt.as_deref().unwrap_or_default();
                            let prompt_ready = !sid.is_empty() && !raw.trim().is_empty();

                            if feature_enabled
                                && prompt_ready
                                && !summarized_sessions.read().contains(sid)
                            {
                                // Mark this session as in-flight so we never re-trigger it.
                                summarized_sessions.write().insert(sid.to_string());
                                let raw_prompt = raw.to_string();
                                let mut store = terminal_store.clone();
                                let pane = pane_id.clone();

                                // Idle -> Pending. The pill renders empty while waiting.
                                {
                                    let mut g = store.write();
                                    if let Some(session) = g.sessions.get_mut(&pane) {
                                        session.title_state =
                                            crate::utils::pane_label::TitleState::Pending;
                                        session.generation = session.generation.wrapping_add(1);
                                    }
                                }

                                // Fire-and-forget. The backend command retries transient
                                // failures internally, so this single await yields a final
                                // result (title, "Sensitive prompt", or Err).
                                wasm_bindgen_futures::spawn_local(async move {
                                    let result = summarize_agent_title(&raw_prompt).await;
                                    let mut g = store.write();
                                    let Some(session) = g.sessions.get_mut(&pane) else {
                                        return;
                                    };
                                    match result {
                                        Ok(summary) => {
                                            let cleaned = summary.trim().to_string();
                                            web_sys::console::log_1(
                                                &format!(
                                                    "[AgentInfoPoller] title for pane={}: {}",
                                                    pane, cleaned
                                                )
                                                .into(),
                                            );
                                            session.title_state =
                                                crate::utils::pane_label::TitleState::Done(cleaned);
                                        }
                                        Err(e) => {
                                            web_sys::console::warn_1(
                                                &format!(
                                                    "[AgentInfoPoller] title failed for pane={}: {:?}",
                                                    pane, e
                                                )
                                                .into(),
                                            );
                                            // Failed is terminal: the pill stays empty.
                                            session.title_state =
                                                crate::utils::pane_label::TitleState::Failed;
                                        }
                                    }
                                    session.generation = session.generation.wrapping_add(1);
                                });
                            }
```

Keep the `terminal_store.write().update_agent_info(...)` call below it (line ~111) unchanged.

- [ ] **Step 3: Add the `ui` import if missing**

The file already imports `use_ui_store`; confirm `ui_state` is in scope (it is, line 35). No import change needed unless the compiler says so.

- [ ] **Step 4: Build (will still fail in terminal_grid.rs until Task 8)**

Run: `cd frontend && cargo check 2>&1 | tail -30`
Expected: errors only in `terminal_grid.rs` (referencing `summarized_title`). Proceed to Task 8.

---

## Task 8: Collapse the `left_label` ladder + truncation/tooltip

**Why:** Spec §5. Replace the 5-tier ladder with the 4-tier match calling `resolve_pane_label`, add view-only truncation + hover tooltip. This completes the frontend wiring and makes the crate compile again. Depends on Tasks 5–7.

**Files:**
- Modify: `frontend/src/components/workspace/terminal_grid.rs:456-492` (the `left_label` block) and the render site that outputs `left_label`.

**Interfaces:**
- Consumes: `resolve_pane_label`, `TitleState` from `crate::utils::pane_label`; `session.title_state` (Task 6); `ui_state.smart_pane_titles` (Task 9).
- Produces: a collapsed ladder and a truncated label string + tooltip for rendering.

- [ ] **Step 1: Replace the `left_label` block**

Read the current block (lines 456–492) and the surrounding code that computes `props.label`, `summarized_title`, `task_title`, `agent_label`, `fg_process` so the replacement uses the right local names. Then replace lines 462–492 with:

```rust
    let left_label = crate::utils::pane_label::resolve_pane_label(
        props.label.as_deref(),
        &title_state,
        &props.agent_type,
        fg_process.as_deref(),
        ui_state.read().smart_pane_titles,
        &agent_label,
    );

    // View-only truncation. The store keeps the full title; the pill shows
    // up to ~24 chars with an ellipsis, full text on hover.
    const LABEL_MAX_CHARS: usize = 24;
    let (display_label, tooltip) = if left_label.chars().count() <= LABEL_MAX_CHARS {
        (left_label.clone(), None)
    } else {
        let truncated: String = left_label.chars().take(LABEL_MAX_CHARS).collect();
        (format!("{}…", truncated), Some(left_label.clone()))
    };
```

Where `title_state` is read from the session near where `summarized_title`/`task_title` were previously read. Replace that read with:

```rust
    let title_state = session.title_state.clone();
```

(taking it from the same `TerminalSession` the old `summarized_title` came from; match the existing borrow pattern). Remove the now-unused `let summarize_active = ui_state.read().summarize_agent_titles;` line and any `task_title` local that was only used for the ladder.

- [ ] **Step 2: Render with the tooltip**

Find the element that renders `{left_label}` in the `rsx!` and replace it with `{display_label}`, and set a `title` attribute for the tooltip on its container:

```rust
                            // within the pane header element:
                            title: tooltip.as_deref().unwrap_or(&display_label),
                            // ...
                            {display_label}
```

(Adapt to the actual element structure — the goal is: visible text is `display_label`; the `title` attribute carries the full `left_label` when truncated, else the label itself. If the container already has a `title`, merge rather than overwrite.)

- [ ] **Step 3: Remove dead references**

Grep for `summarized_title` and `summarize_agent_titles` and `auto_generate_titles` in `terminal_grid.rs` and remove any now-unused reads/locals. (The two store fields are removed in Task 9; their reads here must go now.)

Run: `cd frontend && grep -n "summarized_title\|summarize_agent_titles\|auto_generate_titles" src/components/workspace/terminal_grid.rs`
Expected: no matches (all removed).

- [ ] **Step 4: Build the frontend**

Run: `cd frontend && cargo check 2>&1 | tail -30`
Expected: compiles clean (the store fields referenced — `smart_pane_titles`, `title_state` — exist from Tasks 5–6; the settings field exists after Task 9, so **do Task 9 before Step 5** if `smart_pane_titles` is not yet defined). If `smart_pane_titles` is the only missing symbol, proceed to Task 9 now and return here.

- [ ] **Step 5: Run the full frontend test suite**

Run: `cd frontend && cargo test 2>&1 | tail -30`
Expected: all tests PASS (including the new `pane_label::tests`).

- [ ] **Step 6: fmt + clippy**

Run: `cd frontend && cargo fmt && cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 7: Commit (folds Tasks 6+7+8 into one green commit)**

```bash
git add frontend/src/stores/terminal.rs frontend/src/components/workspace/agent_info_poller.rs frontend/src/components/workspace/terminal_grid.rs
git commit -m "feat: collapsed pane-label ladder + TitleState machine

Replaces summarized_title with a TitleState (Idle/Pending/Failed/Done)
driven by the poller: Idle->Pending on first prompt, Done/Failed on the
backend result (which retries internally). The ladder collapses to a
4-tier resolve_pane_label call; Pending/Failed render an empty pill and
the raw prompt is never a label. View-only truncation (24 chars + tooltip)."
```

---

## Task 9: Merge the two settings into `smart_pane_titles` (store + migration + UI)

**Why:** Spec §1. Replace the two `bool` fields with one, migrate on startup, collapse the two UI toggles into one. This is what makes `smart_pane_titles` exist (unblocking Tasks 7–8 if not already done).

**Files:**
- Modify: `frontend/src/stores/ui.rs:48-78` (fields + Default)
- Modify: `frontend/src/lib.rs:186-192` (migration load)
- Modify: `frontend/src/components/settings/settings_modal.rs:234-312` (two toggles → one)

**Interfaces:**
- Consumes: `tauri_bridge::{store_get, store_set}`.
- Produces: `UIState.smart_pane_titles: bool` (default `true`); a migration read on startup; a single `Smart pane titles` toggle in the UI.

- [ ] **Step 1: Replace the two fields with one in `UIState`**

In `frontend/src/stores/ui.rs`, replace the two field declarations (lines 48–55) with:

```rust
    /// Whether panes get auto-generated titles: agent panes get an LLM
    /// summary of their first prompt; idle Shell panes get a random name.
    /// Migrated on startup from the old `auto_generate_titles` and
    /// `summarize_agent_titles` keys; persisted under `"smart_pane_titles"`.
    pub smart_pane_titles: bool,
```

In `Default for UIState` (lines 77–78), replace the two lines with:

```rust
            smart_pane_titles: true,
```

- [ ] **Step 2: Write the migration as a tested pure fn**

Add a pure migration function + test in `ui.rs` (so the precedence is unit-tested, per spec §7):

```rust
/// Decide the `smart_pane_titles` value from the two legacy setting strings.
/// Precedence: summarize_agent_titles wins; else auto_generate_titles;
/// else false. Returns the new value. Pure and unit-tested.
pub fn migrate_smart_pane_titles(summarize: Option<&str>, auto: Option<&str>) -> bool {
    if summarize.map(|v| v == "true").unwrap_or(false) {
        return true;
    }
    if auto.map(|v| v == "true").unwrap_or(false) {
        return true;
    }
    false
}

#[cfg(test)]
mod migrate_tests {
    use super::migrate_smart_pane_titles;

    #[test]
    fn summarize_wins_regardless_of_auto() {
        assert_eq!(migrate_smart_pane_titles(Some("true"), Some("false")), true);
        assert_eq!(migrate_smart_pane_titles(Some("true"), None), true);
        assert_eq!(migrate_smart_pane_titles(Some("true"), Some("true")), true);
    }

    #[test]
    fn falls_back_to_auto() {
        assert_eq!(migrate_smart_pane_titles(Some("false"), Some("true")), true);
        assert_eq!(migrate_smart_pane_titles(None, Some("true")), true);
    }

    #[test]
    fn both_off_is_false() {
        assert_eq!(migrate_smart_pane_titles(Some("false"), Some("false")), false);
        assert_eq!(migrate_smart_pane_titles(None, None), false);
    }

    #[test]
    fn idempotent() {
        let v = migrate_smart_pane_titles(Some("true"), Some("false"));
        // Running the migration on its own output is a no-op conceptually:
        // once smart_pane_titles is persisted, the legacy keys are ignored.
        assert_eq!(migrate_smart_pane_titles(Some("true"), Some("false")), v);
    }
}
```

- [ ] **Step 3: Run the migration tests**

Run: `cd frontend && cargo test migrate_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Replace the startup load with migration**

In `frontend/src/lib.rs`, replace lines 186–192 with:

```rust
                // Migrate the two legacy pane-title settings into the single
                // `smart_pane_titles` key (once), then read it.
                {
                    let summarize = crate::tauri_bridge::store_get("summarize_agent_titles").await.ok();
                    let auto = crate::tauri_bridge::store_get("auto_generate_titles").await.ok();
                    let migrated = crate::stores::ui::migrate_smart_pane_titles(
                        summarize.as_deref(),
                        auto.as_deref(),
                    );
                    // Read the canonical key if already migrated; else write it.
                    match crate::tauri_bridge::store_get("smart_pane_titles").await {
                        Ok(v) => {
                            ui.write().smart_pane_titles = v == "true";
                        }
                        Err(_) => {
                            // Not yet persisted: persist the migrated value and apply it.
                            let s = if migrated { "true" } else { "false" };
                            let _ = crate::tauri_bridge::store_set("smart_pane_titles", s).await;
                            ui.write().smart_pane_titles = migrated;
                        }
                    }
                }
```

- [ ] **Step 5: Collapse the two UI toggles into one**

In `frontend/src/components/settings/settings_modal.rs`, replace lines 234–312 (the "Pane Titles" `SectionHeader` + first toggle + second toggle) with a single toggle:

```rust
            /* Pane Titles */
            SectionHeader { title: "Pane Titles", desc: "Auto-generated labels above each pane" }
            div {
                style: "display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 0;",
                div {
                    style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
                    div {
                        style: "font-size: var(--text-sm); font-weight: 600; color: var(--text);",
                        "Smart pane titles"
                    }
                    div {
                        style: "font-size: var(--text-xs); color: var(--textMuted);",
                        "Auto-generate a short title for each agent pane using the configured LLM. Idle shells keep a random name."
                    }
                }
                {
                    let enabled = ui_state.read().smart_pane_titles;
                    let bg = if enabled { "var(--accent)" } else { "var(--bgTertiary)" };
                    let knob = if enabled { "translateX(20px)" } else { "translateX(0px)" };
                    rsx! {
                        button {
                            style: "position: relative; width: 44px; height: 24px; border-radius: 999px; border: 1px solid var(--border); background: {bg}; cursor: pointer; padding: 0; flex-shrink: 0; transition: background 0.15s ease;",
                            onclick: move |_| {
                                let next = !ui_state.read().smart_pane_titles;
                                ui_state.write().smart_pane_titles = next;
                                spawn(async move {
                                    let _ = crate::tauri_bridge::store_set(
                                        "smart_pane_titles",
                                        if next { "true" } else { "false" },
                                    )
                                    .await;
                                });
                            },
                            div {
                                style: "position: absolute; top: 1px; left: 1px; width: 20px; height: 20px; border-radius: 50%; background: var(--bg); transform: {knob}; transition: transform 0.15s ease;",
                            }
                        }
                    }
                }
            }
```

- [ ] **Step 6: Build + test the frontend**

Run: `cd frontend && cargo check 2>&1 | tail -30`
Expected: compiles clean.

Run: `cd frontend && cargo test 2>&1 | tail -30`
Expected: all tests PASS.

- [ ] **Step 7: fmt + clippy**

Run: `cd frontend && cargo fmt && cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/stores/ui.rs frontend/src/lib.rs frontend/src/components/settings/settings_modal.rs
git commit -m "feat: merge pane-title settings into one smart_pane_titles toggle

Replaces auto_generate_titles + summarize_agent_titles with a single
smart_pane_titles toggle (default true). Migrates the two legacy keys
on startup via a tested pure fn, then reads/writes smart_pane_titles.
UI collapses the two toggles into one \"Smart pane titles\"."
```

---

## Task 10: Build the frontend dist + full-workspace verification + manual smoke test

**Why:** The plan touches both the backend (Rust) and the Dioxus WASM frontend. The frontend must be rebuilt into `dist/` for the app to pick up the changes, and the whole workspace must build clean. Then a manual smoke test confirms the user-visible behavior (titles appear, no raw-prompt flash, empty-while-pending).

**Files:** none modified — build + verify only.

- [ ] **Step 1: Full workspace check + test**

Run: `cargo check --workspace 2>&1 | tail -20`
Expected: compiles clean.

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: all tests PASS across `athena-core`, `src-tauri`, and `frontend` (host-native frontend tests run despite the wasm target).

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -20`
Expected: clean. Fix any warnings.

Run: `cargo fmt --check 2>&1 | tail -5` (or `cargo fmt` to apply)
Expected: no diffs.

- [ ] **Step 2: Build the frontend dist (release, required for the app to load under WKWebView)**

Run: `bash frontend/build-dist.sh 2>&1 | tail -20`
Expected: builds the WASM, copies to `dist/`, reports success. (Per CLAUDE.md: release build, no `--debug` flag — debug builds hit the Dioxus devtools WebSocket panic in WKWebView.)

- [ ] **Step 3: Build the debug app binary**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: compiles clean.

- [ ] **Step 4: Run the app and smoke-test the four original bugs**

Run: `cargo run --manifest-path src-tauri/Cargo.toml &` (or `cargo tauri dev`).

Manually verify (the user's original symptoms):
1. **Settings:** General → "Pane Titles" shows ONE toggle, "Smart pane titles". Turning it off hides all pane titles (static labels everywhere, no random shell names).
2. **Claude pane:** open a Claude pane, type a prompt like "analyze the codebase". While the LLM call is in flight the pill is **empty** (no "analyze the codebase" raw-prompt flash). When it lands, the pill shows a short `-ing` title (e.g. "analyzing the codebase") — no "ready"-style padding.
3. **Codex pane:** open a Codex pane, type a prompt. The pill now gets a title (previously stuck on the raw thread name). Verify `~/.codex/history.jsonl` is being read (a fresh prompt updates it).
4. **Truncation:** type a long prompt; confirm the pill truncates with `…` and the full title appears on hover.
5. **No-API-key path:** with no LLM key configured, a Claude pane's pill stays empty (Pending → Failed), not "Untitled".
6. **Sensitive prompt:** type a prompt containing "password"; pill shows "Sensitive prompt" (the one non-empty exception), no LLM call made.

- [ ] **Step 5: Kill the running app**

Run: `pkill -f "athena-core" || pkill -f "cargo run"` (or close the window).

- [ ] **Step 6: Final commit if any formatting/build artifacts need staging**

(Usually nothing to stage after a build — `dist/` is likely gitignored. Confirm: `git status`.)

Run: `git status`
Expected: clean working tree (or only pre-existing untracked files unrelated to this change).

- [ ] **Step 7: Final summary commit if any docs need a pointer (optional)**

If the spec's "Out of scope" or behavior should be noted in CLAUDE.md, skip — the spec doc already records it. No commit needed unless the user requests doc updates.

---

## Self-Review (run after writing, before handoff)

**1. Spec coverage:**
- §1 merged setting + migration → Task 9. ✓
- §2 prompt wording + `max_tokens` 48 → Task 2 Step 3. ✓
- §3 lifecycle (state machine, empty-while-pending, retry-with-backoff, sensitive exception) → Tasks 2, 3, 5, 6, 7. ✓
- §4 Codex fix + Claude hygiene → Task 4. ✓
- §5 collapsed ladder + truncation/tooltip → Tasks 5, 8. ✓
- §6 error handling (retry ceiling, missing-key non-retryable, empty-prompt guard, user-rename-wins) → Tasks 2, 3, 7, 5. ✓
- §7 testing (backend retry tests, scraper tests, sensitive-filter tests, pure-fn ladder tests, migration tests) → Tasks 2, 3, 4, 5, 9. ✓
- Out-of-scope items (chat-panel session titles, poll interval, failure surfacing) → deliberately untouched. ✓

**2. Placeholder scan:** No "TBD"/"TODO" in steps. The two "read the actual function and edit minimally" notes (Task 6 Step 3, Task 8 Step 1) are intentional — they guard against the plan lying about exact surrounding code that may have drifted; they name the precise symbol and intent. Acceptable.

**3. Type/signature consistency:**
- `TitleState` defined in Task 5, used identically in Tasks 6, 7, 8. ✓
- `resolve_pane_label(label: Option<&str>, title_state: &TitleState, agent_type: &AgentType, fg_process: Option<&str>, smart_on: bool, static_agent_label: &str)` — defined Task 5, called Task 8 with matching args. ✓
- `summarize_title` returns `Result<String, OrchestratorError>`; `summarize_agent_title` maps `Err(MissingApiKey)` → `Err(String)`. ✓
- `smart_pane_titles` field on `UIState` — defined Task 9, read in Tasks 7, 8. **Order note:** Task 9 must land before Tasks 7/8 compile; the plan calls this out explicitly in Task 8 Step 4 and Task 9's "unblocking" framing. If executing strictly in order, do Task 9's Step 1 (field) early or reorder 9 before 7/8. The plan flags this. ✓ (flagged)
- `parse_codex_history` / `parse_claude_history` — defined Task 4 Step 3, tested Task 4 Step 1. ✓
- `prompt_is_sensitive` — defined Task 3, tested Task 3. ✓
- `migrate_smart_pane_titles` — defined Task 9 Step 2, tested same. ✓
- `new_for_test` + `summarize_title_for_test` — defined Task 1/2, used Task 2 tests. ✓

**One ordering risk flagged for the executor:** Tasks 7 and 8 reference `smart_pane_titles` before Task 9 defines it. Recommended execution order to stay green between tasks: **1 → 2 → 3 → 4 → 5 → 9 → 6 → 7 → 8 → 10.** (Task 9's field + migration before the store/poller/ladder consume it.) The inline notes already steer this; the executor should follow that order.
