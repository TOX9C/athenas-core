//! Local-network discovery for the Athena relay.
//!
//! The desktop advertises a stable service type so native companion clients can
//! browse nearby instances with Bonjour/mDNS. Browsers cannot enumerate mDNS
//! services directly, so the relay also exposes a tiny descriptor endpoint for
//! QR/manual connection flows and future native bridges.

use std::collections::HashMap;
use std::net::SocketAddr;

use serde::Serialize;

pub const SERVICE_TYPE: &str = "_athena._tcp.local.";

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryInfo {
    pub service: &'static str,
    pub name: String,
    pub port: u16,
    #[serde(rename = "httpPath")]
    pub http_path: &'static str,
    #[serde(rename = "wsPath")]
    pub ws_path: &'static str,
    pub protocol: &'static str,
    pub version: &'static str,
    #[serde(rename = "requiresToken")]
    pub requires_token: bool,
}

pub fn info(addr: SocketAddr) -> DiscoveryInfo {
    DiscoveryInfo {
        service: SERVICE_TYPE,
        name: instance_name(),
        port: addr.port(),
        http_path: "/",
        ws_path: "/ws",
        protocol: "athena-relay-v1",
        version: env!("CARGO_PKG_VERSION"),
        requires_token: true,
    }
}

/// Start advertising the relay. mDNS failures are deliberately non-fatal: a
/// firewall, VPN, or unsupported network should not prevent QR/manual links
/// from working.
pub fn advertise(addr: SocketAddr) -> Option<mdns_sd::ServiceDaemon> {
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(daemon) => daemon,
        Err(error) => {
            log::warn!("[relay] mDNS daemon unavailable: {error}");
            return None;
        }
    };

    let host = host_label();
    let instance = instance_name();
    let mut properties = HashMap::new();
    properties.insert("path".to_string(), "/".to_string());
    properties.insert("ws".to_string(), "/ws".to_string());
    properties.insert("protocol".to_string(), "athena-relay-v1".to_string());
    properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    properties.insert("token".to_string(), "required".to_string());

    let service = match mdns_sd::ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &format!("{host}.local."),
        "",
        addr.port(),
        Some(properties),
    ) {
        Ok(service) => service,
        Err(error) => {
            log::warn!("[relay] mDNS service description failed: {error}");
            return None;
        }
    };

    if let Err(error) = daemon.register(service) {
        log::warn!("[relay] mDNS registration failed: {error}");
        return None;
    }

    log::info!("[relay] advertising {instance} as {SERVICE_TYPE}");
    Some(daemon)
}

fn host_label() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "athena-core".to_string())
        .trim_end_matches('.')
        .split('.')
        .next()
        .unwrap_or("athena-core")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
        .if_empty_then("athena-core")
}

fn instance_name() -> String {
    format!("Athena's Core — {}", host_label())
}

trait EmptyStringFallback {
    fn if_empty_then(self, fallback: &str) -> String;
}

impl EmptyStringFallback for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
