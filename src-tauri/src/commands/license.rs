//! License-key activation (Lemon Squeezy) for commercial use under BUSL-1.1.
//!
//! Hard product rules (CLAUDE.md): activate ONCE, store locally, OFFLINE
//! FOREVER after activation; NEVER gate or degrade a running app on license
//! status — licensing is visible in settings, never a wall. `license_status`
//! therefore reads only the local store and never touches the network.
//!
//! Persistence mirrors the other settings: the license record lives in the
//! shared `athena_store::KeyValueStore` (`state.store`) under the
//! `license.data` key — the same mechanism `store_set`/`store_delete` use,
//! no separate file.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::CommandError;

const STORE_KEY: &str = "license.data";
const API_BASE: &str = "https://api.lemonsqueezy.com/v1/licenses";

/// Locally persisted activation record — everything needed offline forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicenseRecord {
    key: String,
    instance_id: String,
    product: String,
    customer_email: String,
    activated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
struct ApiResponse {
    activated: Option<bool>,
    error: Option<String>,
    instance: Option<ApiInstance>,
    meta: Option<ApiMeta>,
}

#[derive(Debug, Deserialize)]
struct ApiInstance {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ApiMeta {
    product_name: Option<String>,
    customer_email: Option<String>,
}

fn read_record(store: &athena_store::KeyValueStore) -> Option<LicenseRecord> {
    store.get::<LicenseRecord>(STORE_KEY).ok().flatten()
}

fn status_json(record: Option<LicenseRecord>) -> serde_json::Value {
    match record {
        Some(r) => serde_json::json!({
            "activated": true,
            "product": r.product,
            "customer_email": r.customer_email,
        }),
        None => serde_json::json!({ "activated": false }),
    }
}

/// Machine label used as the Lemon Squeezy instance name. Mirrors
/// `relay::discovery::host_label`'s std-only approach (HOSTNAME /
/// COMPUTERNAME env, trimmed to the first DNS label).
fn instance_name() -> String {
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "athena-core".to_string());
    let label = host.trim_end_matches('.').split('.').next().unwrap_or("").trim();
    if label.is_empty() {
        "athena-core".to_string()
    } else {
        label.to_string()
    }
}

async fn post(endpoint: &str, form: &[(&str, &str)]) -> Result<ApiResponse, CommandError> {
    let url = format!("{API_BASE}/{endpoint}");
    let resp = reqwest::Client::new()
        .post(&url)
        .form(form)
        .send()
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "Could not reach the license server (are you offline?): {e}"
            ))
        })?;
    let body = resp.text().await.map_err(|e| {
        CommandError::Internal(format!("License server response unreadable: {e}"))
    })?;
    serde_json::from_str(&body)
        .map_err(|e| CommandError::Internal(format!("License server response invalid: {e}")))
}

/// Activate a license key against Lemon Squeezy and persist the record
/// locally. One-time network call — after this the app is offline-forever.
#[tauri::command]
pub async fn license_activate(
    state: State<'_, AppState>,
    key: String,
) -> Result<serde_json::Value, CommandError> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(CommandError::InvalidInput(
            "License key must not be empty".to_string(),
        ));
    }
    let resp = post("activate", &[
        ("license_key", key.as_str()),
        ("instance_name", &instance_name()),
    ])
    .await?;
    if resp.activated != Some(true) {
        return Err(CommandError::InvalidInput(
            resp.error
                .filter(|e| !e.trim().is_empty())
                .unwrap_or_else(|| "License key could not be activated".to_string()),
        ));
    }
    let instance_id = resp
        .instance
        .map(|i| i.id)
        .ok_or_else(|| CommandError::Internal("Activation succeeded but no instance id returned".into()))?;
    let meta = resp.meta.unwrap_or(ApiMeta {
        product_name: None,
        customer_email: None,
    });
    let record = LicenseRecord {
        key,
        instance_id,
        product: meta
            .product_name
            .unwrap_or_else(|| "Athena's Core".to_string()),
        customer_email: meta.customer_email.unwrap_or_default(),
        activated_at: chrono_now_iso(),
    };
    state
        .store
        .set_sync(STORE_KEY, &record)
        .map_err(|e| CommandError::Internal(format!("Failed to persist license: {e}")))?;
    log::info!("[license] activated for {}", record.customer_email);
    Ok(status_json(Some(record)))
}

/// Deactivate: best-effort network call, then always remove the local copy.
#[tauri::command]
pub async fn license_deactivate(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CommandError> {
    if let Some(record) = read_record(&state.store) {
        if let Err(e) = post("deactivate", &[
            ("license_key", record.key.as_str()),
            ("instance_id", record.instance_id.as_str()),
        ])
        .await
        {
            // Offline-forever: the remote call may fail; local copy goes anyway.
            log::warn!("[license] remote deactivate failed (removing local copy anyway): {e}");
        }
        state
            .store
            .delete_sync(STORE_KEY)
            .map_err(|e| CommandError::Internal(format!("Failed to remove local license: {e}")))?;
        log::info!("[license] deactivated");
    }
    Ok(status_json(None))
}

/// Current license status. LOCAL READ ONLY — never calls the network
/// (offline-forever rule; a manual re-check may be added later, separately).
#[tauri::command]
pub async fn license_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CommandError> {
    Ok(status_json(read_record(&state.store)))
}

/// std-only RFC3339-ish UTC timestamp (no chrono dep in this crate).
fn chrono_now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // ponytail: civil-from-days math instead of pulling chrono/time for one
    // informational timestamp; formatting only, never parsed.
    let days = (secs / 86_400) as i64;
    let mut y = 1970i64;
    let mut rem = days;
    loop {
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let len = if leap { 366 } else { 365 };
        if rem < len {
            break;
        }
        rem -= len;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let mdays = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut m = 0usize;
    for (i, &d) in mdays.iter().enumerate() {
        if rem < d {
            m = i;
            break;
        }
        rem -= d;
    }
    let day = rem + 1;
    let t = secs % 86_400;
    format!(
        "{y:04}-{:02}-{day:02}T{:02}:{:02}:{:02}Z",
        m + 1,
        t / 3600,
        (t % 3600) / 60,
        t % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_well_formed() {
        let ts = chrono_now_iso();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn status_json_matches_contract() {
        let off = status_json(None);
        assert_eq!(off, serde_json::json!({ "activated": false }));
        let on = status_json(Some(LicenseRecord {
            key: "k".into(),
            instance_id: "i".into(),
            product: "P".into(),
            customer_email: "e@x".into(),
            activated_at: "2026-01-01T00:00:00Z".into(),
        }));
        assert_eq!(on["activated"], true);
        assert_eq!(on["product"], "P");
        // Never leak the raw key through status.
        assert!(on.get("key").is_none());
    }
}
