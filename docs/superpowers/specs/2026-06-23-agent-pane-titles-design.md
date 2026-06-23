# Agent Pane Titles — Design

**Date:** 2026-06-23
**Status:** Approved (brainstorm)
**Scope:** Restructure the agent-pane title system: merge two overlapping settings into one, fix the "whole prompt flashes as the title" bug, fix Codex never being summarized, fix the count-padding wording, and make title behavior consistent per pane type.

## Context & Root Causes

The pane-title system had four observed bugs. Each traces to a concrete root cause:

1. **"code analysis ready" (noisy 3rd word)** — the LLM prompt was `"Summarize in 2-3 words: {prompt}"`. The "2-3 words" target makes the model *pad* toward the count with filler like "ready". (`crates/athena-core/src/orchestrator.rs:911`, `max_tokens: 20`)
2. **Whole prompt becomes the title ("hi", "what rust version…")** — the frontend label priority ladder included `task_title` as a fallback tier, and for Claude `task_title ≡ raw_prompt` (both are `entry.display`). When `summarize_agent_titles` was ON but the fire-and-forget LLM call hadn't landed (or had failed), the raw prompt rendered as the label. (`frontend/src/components/workspace/terminal_grid.rs:462`, `src-tauri/src/commands/mod.rs:1838`)
3. **"Two settings that do the same thing"** — `auto_generate_titles` and `summarize_agent_titles` overlap. `summarize_agent_titles` is really just an *upgrade* to `auto_generate_titles`, but the UI presented them as flat peers under "Pane Titles" with no hierarchy.
4. **Not agent-specific / Codex silently skipped** — the summarizer only fired when `!sid.is_empty()` (`agent_info_poller.rs:72`). Codex's scraper returned `session_id=None, raw_prompt=None` (it read the wrong file — `session_index.jsonl`'s `thread_name`, a thread name, not a prompt), so Codex panes never entered the summarization path and were stuck on the raw scraped string forever.

**Key data finding:** for Codex, `session_id` and the raw prompt are *not* in the file currently read (`~/.codex/session_index.jsonl`), but **are** present in a sibling file `~/.codex/history.jsonl` (`session_id`, `text`, `ts` fields). The Codex fix is therefore an extension of the scraper, not blocked on missing data.

## Decisions (from brainstorm)

| Decision | Choice |
|---|---|
| Settings structure | Merge into one toggle `smart_pane_titles`; migrate the two old keys |
| Title style | Short sentence, sentence case, 1–6 words, `-ing`/imperative form |
| Over-length handling | Truncate to ~24 chars + ellipsis, full text in hover tooltip |
| No-LLM / pending / failed fallback | Leave pill empty; no raw-prompt fallback, no "Untitled" |
| Retry on error | Retry-with-backoff (1s→2s→4s, max 3 attempts, ~7s ceiling), then empty |
| Per-agent coverage | Agents only (Claude + Codex); idle Shell keeps random name |
| Architecture | A — Backend owns the full title lifecycle |

## Section 1 — The merged setting

**New key:** `smart_pane_titles` (default `true`). The two old keys (`auto_generate_titles`, `summarize_agent_titles`) are read once at startup for **migration**:
- if `summarize_agent_titles == true` → `true`
- else if `auto_generate_titles == true` → `true`
- else `false`

After migration the old keys are ignored (left in place; harmless, and makes the migration idempotent / crash-safe).

**UI** (`frontend/src/components/settings/settings_modal.rs`, "Pane Titles" section) — a single toggle:
- **Label:** `Smart pane titles`
- **Desc:** `Auto-generate a short title for each agent pane using the configured LLM. Idle shells keep a random name.`

The desc makes the LLM's role explicit without a second toggle. **Shell random names** move under this same single flag (previously gated on `auto_generate_titles`).

**Semantics:**
- **ON:** Claude & Codex panes get an LLM title; idle Shell panes get a random name; other pane types show their static label.
- **OFF:** no titles at all (static label everywhere, no random shell names).
- No separate "no API key" toggle — handled gracefully at runtime (→ empty while pending, §3).

## Section 2 — The LLM prompt + wording

**Source text:** the agent's scraped raw prompt (first user message), never conversation history. Claude → `~/.claude/history.jsonl` `display`; Codex → `~/.codex/history.jsonl` `text` (the fix, §4). One prompt in, one title out.

**New prompt** (`crates/athena-core/src/orchestrator.rs:911`), replacing the count-padded `"Summarize in 2-3 words"`:

```
SYSTEM: You write short, descriptive titles for coding sessions based on the user's first prompt.
Write a short sentence in sentence case describing what the agent is doing, 1–6 words.
Use the imperative or -ing form (e.g. "analyzing the codebase", "checking rust version", "fixing the login bug").
No quotes, no trailing punctuation, no preamble — output only the title.

USER: {raw_prompt}
```

Rationale:
- "Short sentence, sentence case, 1–6 words" matches the chosen style and gives the model room where a 4-word thought is natural instead of padding to hit "2–3".
- "-ing / imperative form" drives verbs (analyzing, checking, fixing) rather than noun fragments and avoids the "ready" filler.
- "Output only the title" prevents `Title: ` prefix bleed-through.

**`max_tokens`:** raised `20 → 48` to avoid truncating a legit 4–6 word sentence mid-word.

**Sensitive-prompt filter** stays as-is (`src-tauri/src/commands/mod.rs:2144–2172`) — runs *before* the LLM call, short-circuits to `"Sensitive prompt"`. This is a deliberate non-empty exception (see §3).

## Section 3 — The lifecycle (empty-while-pending + retry-with-backoff)

Backend owns the whole lifecycle (Approach A). This is where the "whole prompt flashes" bug dies at the root: the frontend never uses `raw_prompt`/`task_title` as a label, so there is no code path that can render the raw prompt regardless of timing.

**Per-pane state machine** (frontend `TerminalSession`), replacing the single `summarized_title: Option<String>`:

```
TitleState ::= Idle | Pending | Failed | Done(String)
```

(The backend owns the entire retry loop — see below — so the frontend never has "retries left"; `Failed` is terminal.)

- **Idle** — no prompt scraped yet. Pill shows the static agent label (e.g. "Claude Code"). Shell panes never enter this machine — they take the random-name branch.
- **Pending** — a prompt was scraped, the LLM call is in flight (the backend is retrying internally). Pill shows **empty** (no label text), until the result lands.
- **Failed** — the backend exhausted its retries (or hit a non-retryable error). Pill stays **empty**.
- **Done(title)** — success. Pill shows `title`.

**Backend retry** (`crates/athena-core/src/orchestrator.rs`, new `summarize_title` body). The command does the retry loop so the frontend's fire-and-forget call just awaits a final answer:

```
summarize_title(raw_prompt):
    for attempt in 1..=MAX_ATTEMPTS (3):
        result = do_one_llm_call(...)
        if Ok(t): return Ok(t)
        if Err(retryable): sleep(backoff_for(attempt)); continue   // 1s, 2s, 4s
        if Err(non_retryable): return Err(...)                      // sensitive-filter, missing key
    return Err("title generation failed after retries")
```

- **Retryable:** network / 5xx / timeout / parse error → backoff 1s → 2s → 4s, max 3 attempts (~7s ceiling).
- **Non-retryable (fail fast):**
  - Sensitive-prompt match → `"Sensitive prompt"` → frontend `Done` (the documented non-empty exception; avoids burning calls on deliberately-rejected prompts and gives the user a visible redaction signal).
  - Missing API key → return a sentinel → frontend `Failed` → pill empty. (Replaces the old `Ok("Untitled")` behavior; correct per §3.)

**Frontend wiring** (`frontend/src/components/workspace/agent_info_poller.rs`): on detecting a new `session_id` (or, for Codex, a new `raw_prompt` — §4), transition `Idle → Pending` and fire the single `summarize_agent_title` call. The command blocks internally through its retries; on return, the poller sets `Done(title)` or `Failed`. The `summarized_sessions` HashSet is retained as the "already titled this session" guard so we never re-call for the same session. It is **not** cleared on failure (avoid hammering the LLM every 1500ms for a persistently-failing pane).

**Truncation** (frontend view layer): the store holds the full title; the pill truncates to ~24 chars + ellipsis, full text on hover tooltip. Truncation is view-only, never stored.

## Section 4 — Agent-specific scraping (Codex fix + Claude hygiene)

**Codex fix** (`scrape_codex_task`, `src-tauri/src/commands/mod.rs:1715`). Today it reads only `~/.codex/session_index.jsonl` → `thread_name`, and the caller hardcodes `session_id=None, raw_prompt=None`. Extend it to return a struct mirroring `ClaudeHistoryEntry` by also reading `~/.codex/history.jsonl`:

```
CodexHistoryEntry { display, session_id, timestamp }
```
- `session_id` ← `history.jsonl` `session_id` field
- `display` ← `history.jsonl` `text` field (the user's actual prompt)
- `timestamp` ← `history.jsonl` `ts` field

The caller's Codex arm then mirrors Claude's: `(Some(display), Some(session_id), Some(timestamp), Some(raw_prompt))`. The `session_index.jsonl` `thread_name` path is **dropped** — it was the wrong source (a thread name, not a prompt).

**Claude hygiene** (`scrape_claude_task`, `src-tauri/src/commands/mod.rs:1697`):
- `entry.display` was used as **both** `task_title` and `raw_prompt` — that conflation made the scraped fallback look identical to the raw prompt. After §3 there is no scraped-fallback label path, so `task_title` (the display string) becomes **unused for labeling**. Kept in `AgentInfo` for debugging/badge use; it no longer feeds the label ladder. The only field that matters for titles is `raw_prompt`.
- Harden the parse: a malformed last line makes the whole scrape return `None` (no title). Keep failing closed.

**Per-agent summary table:**

| Pane type | Title source | Behavior |
|---|---|---|
| Claude | `~/.claude/history.jsonl` `display` | LLM title (state machine) |
| Codex | `~/.codex/history.jsonl` `text` (NEW) | LLM title (state machine, NEW) |
| Idle Shell | — | random `name_for_pane` |
| Running non-agent (vim, etc.) | — | static label + process badge |
| Idle Shell with `smart_pane_titles` OFF | — | static label |

## Section 5 — The collapsed frontend label ladder

**Today** (5 tiers with type-checks sprinkled in, and the raw-prompt leak):
```
user-rename > LLM summary > scraped task_title (raw prompt!) > random shell name > static
   ↑ feature ON branches one way, OFF branches another, with Shell checks inside both
```

**After** (the `task_title`/`raw_prompt` label path is gone entirely):
```
let left_label = match (&props.label, &title_state, agent_type, fg_process, smart_on) {
    // 1. User rename always wins
    (Some(label), ..) => label.clone(),

    // 2. Idle Shell → random name (only when feature ON)
    (_, _, Shell, idle, true) => name_for_pane(&pane_id),

    // 3. Agent pane with a title → render its TitleState
    (_, title_state, agent_type, ..) => render_title_state(title_state, agent_type),

    // 4. Everything else → static label
    _ => agent_label,
};
```

Where `render_title_state` maps the §3 states to visible text:
- `Idle` → static agent label ("Claude Code" / "Codex")
- `Pending` → **empty string**
- `Failed` → **empty string**
- `Done(title)` → `title`
- `Done("Sensitive prompt")` → "Sensitive prompt" (documented exception)

OFF means: no random shell names, and agent panes never produce a `Done` state because the poller checks the flag before entering `Pending`.

**Tooltip / truncation** lives in the view layer (the `rsx` that renders `left_label`), not in the ladder. The store keeps the full title; CSS handles `text-overflow: ellipsis` + `title="..."` hover.

## Section 6 — Error handling & edge cases

- **Network / 5xx / timeout (retryable):** backend retry loop, up to 3 attempts, backoff 1s→2s→4s (~7s ceiling), then `Err`. Frontend → `Failed` → pill empty. The `summarized_sessions` guard is not cleared on failure (no per-poll hammering). A pane whose LLM failed 3× stays empty; the user can rename manually or a new prompt creates a new session_id and re-enters the machine.
- **Missing API key (non-retryable):** `summarize_agent_title` returns a sentinel → `Failed` → pill empty. Replaces the old `Ok("Untitled")`. Correct per §3.
- **Malformed history files:** scraper returns `None` → `AgentInfo` fields `None` → poller sees no `raw_prompt` → pane stays `Idle` (static label), never `Pending`. Fail closed.
- **Empty / whitespace-only prompt:** poller guard — if `raw_prompt.trim().is_empty()`, stay `Idle`, don't transition to `Pending`. Prevents a wasted LLM call and an empty title result.
- **Sensitive prompt:** filter runs first, returns `Ok("Sensitive prompt")` immediately, no LLM call, no retry → `Done("Sensitive prompt")` → pill shows "Sensitive prompt". The one non-empty exception.
- **User rename during Pending:** the ladder checks `props.label` first (tier 1), so a manual rename wins and is never overwritten by a late-landing LLM result.
- **Migration race on startup:** old keys read once, new key written, old keys left in place. Idempotent — running twice yields the same `smart_pane_titles`. Crash-safe.
- **WASM crash guard:** retry/backoff happens in the *backend* command (not a WASM `spawn_local` loop), sidestepping the documented Dioxus 0.7 "RuntimeError: Unreachable" risk from complex async in event handlers. The frontend's `spawn_local` just `await`s one command and writes the result — minimal WASM surface.

## Section 7 — Testing

Targeted at where the risk lives: lifecycle and parsing logic, not the LLM call itself.

**Backend unit tests** (`crates/athena-core/src/orchestrator.rs` `#[cfg(test)]`):
- `summarize_title_retries_on_5xx_then_succeeds` — mock HTTP returns 503 twice then 200; assert 3 attempts, returns the title.
- `summarize_title_fails_after_max_attempts` — persistent 503; assert `Err` after exactly 3 attempts, no 4th.
- `summarize_title_non_retryable_missing_key` — no provider config; assert immediate `Err`, no retry attempts consumed.
- `summarize_title_trims_output` — mock returns `"  analyzing the codebase  "`; assert trimmed.

**Scraper tests** (`src-tauri/src/commands/mod.rs` `#[cfg(test)]`):
- `scrape_codex_task_extracts_session_and_prompt` — temp `history.jsonl` fixture with `session_id`, `text`, `ts`; assert all three extracted. (Core of the Codex fix — must have a test.)
- `scrape_codex_task_returns_none_on_malformed` — broken JSON last line → `None`.
- `scrape_claude_task_parses_display_session_timestamp` — fixture with `display`/`sessionId`/`timestamp` → correct `ClaudeHistoryEntry`.

**Sensitive-filter tests:**
- `sensitive_prompt_blocks_variants` — parameterized over `"password"`, `"p@ssword"`, `"API_KEY"`, `"t0k3n"` → all return `Ok("Sensitive prompt")` without calling the LLM.
- `normal_prompt_passes_filter` — `"analyze the codebase"` → proceeds to the LLM path (mocked).

**Frontend — collapsed ladder:** extract the `left_label` match into a pure function `resolve_pane_label(label, title_state, agent_type, fg_process, smart_on) -> String`, then unit-test the truth table:
- user rename present → rename (regardless of title state)
- `Idle` agent → static label
- `Pending` → `""`
- `Failed` → `""`
- `Done("Sensitive prompt")` → `"Sensitive prompt"`
- idle Shell + smart_on → random name
- idle Shell + smart_off → static label

Extracting it as a pure function is the testing move AND aligns with the coding-style rule (small, testable units).

**Migration tests:**
- `migrate_titles_prefers_summarize_flag` — `summarize_agent_titles=true` → `true` regardless of `auto_generate_titles`.
- `migrate_titles_falls_back_to_auto` — `summarize=false`, `auto=true` → `true`.
- `migrate_titles_both_off` → `false`.
- `migrate_idempotent` — run twice, same result.

**Deliberately NOT tested:**
- Real LLM calls — mocked HTTP; the prompt wording can't be meaningfully asserted (non-deterministic model output).
- The Dioxus render of the pill — visual/truncation is view-layer; manual check + existing E2E harness if needed.
- `AgentInfoPoller` timing — the 1500ms loop is infrastructure; the state transitions it triggers are tested via the pure `resolve_pane_label` + backend command tests.

## Out of scope

- Renaming the Athena **chat** panel sessions (`SessionStore` / `session_update`) — those have their own title field and are unrelated to the agent-pane pills this spec covers.
- Changing the poll interval (1500ms) or the `AgentInfoPoller` architecture.
- Surfacing the title-generation failure as a visible error/toast — empty pill is the only signal by design.
- Titles for panes running non-Claude/Codex node tools (e.g. generic `node`) — they keep the static label + process badge.
