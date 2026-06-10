# FIX SPEC: Workstream 5 — API Key & Secret Handling

## Background
API keys are stored and transmitted as plain `String`, making them vulnerable to exposure.
- AGENT_01 [C3]: HTTP API Key Leaks in Error Messages and Headers
- AGENT_07 [Finding 7]: API key loaded into plain `String`, not zeroized
- AGENT_08 [M-12]: API key stored in plain `use_signal(String)`
- AGENT_10 [Finding 19]: `store_api_key` and `clear_api_key` security
- AGENT_10 [Finding 4]: `ProviderConfig` `Clone` potentially leaking keys

## Key Changes
1. **Backend**: Use `secrecy::SecretString` or a zero-on-drop container for API keys in `ProviderConfig` and `orchestrator.rs`.
2. **Backend**: Sanitize all error responses from LLM providers to ensure no key leakage.
3. **Backend**: Update `store_api_key` to use the `keyring` crate instead of plain storage.
4. **Frontend**: Store only a reference or label for the key, not the raw string. Clear sensitive inputs after use.
5. **Audit**: Ensure `ProviderConfig::Clone`, `Debug`, and `Display` do not expose the key.

## Files to Modify
- `crates/athena-core/src/orchestrator.rs`
- `src-tauri/src/commands/athena.rs`
- `frontend/src/components/settings/settings_modal.rs`
- `src-tauri/src/main.rs`

## Verification
- Unit tests verifying keys are redacted in logs and errors.
- Memory tests for zeroization on drop.
