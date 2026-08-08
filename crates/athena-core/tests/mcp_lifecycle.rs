//! Focused lifecycle tests for the Rust MCP TCP transport.

use athena_core::mcp::McpServer;
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn port_is_released(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

async fn wait_for_port_release(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if port_is_released(port) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("MCP port {port} was not released before the timeout");
}

#[tokio::test]
async fn init_is_idempotent_for_the_active_port() {
    let mut server = McpServer::new();
    server.init(0).expect("initial MCP bind should succeed");
    let port = server.port().expect("MCP should expose its bound port");

    server
        .init(port)
        .expect("initializing the active port should be idempotent");
    assert_eq!(server.port(), Some(port));

    server.shutdown();
    wait_for_port_release(port).await;
}

#[tokio::test]
async fn init_rejects_a_different_active_port() {
    let mut server = McpServer::new();
    server.init(0).expect("initial MCP bind should succeed");
    let active_port = server.port().expect("MCP should expose its bound port");
    let requested_port = if active_port == u16::MAX {
        u16::MAX - 1
    } else {
        active_port + 1
    };

    let error = server
        .init(requested_port)
        .expect_err("a different active port must be rejected");
    assert!(error.to_string().contains("already listening"));
    assert_eq!(server.port(), Some(active_port));

    server.shutdown();
    wait_for_port_release(active_port).await;
}

#[tokio::test]
async fn init_reports_an_occupied_port_without_losing_retryability() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).expect("test port bind should succeed");
    let port = occupied
        .local_addr()
        .expect("test listener should have an address")
        .port();
    let mut server = McpServer::new();

    assert!(server.init(port).is_err());
    assert_eq!(server.port(), None);

    drop(occupied);
    server
        .init(port)
        .expect("MCP should retry successfully after the port is released");
    server.shutdown();
    wait_for_port_release(port).await;
}

#[tokio::test]
async fn shutdown_releases_the_tcp_port() {
    let mut server = McpServer::new();
    server.init(0).expect("initial MCP bind should succeed");
    let port = server.port().expect("MCP should expose its bound port");

    server.request_shutdown();
    assert!(server.wait_for_tcp_shutdown().await);
    server.shutdown();
    assert_eq!(server.port(), None);

    // A fresh server can reclaim the port immediately after the original
    // listener generation reports that it has exited.
    let mut replacement = McpServer::new();
    replacement
        .init(port)
        .expect("released MCP port should be reusable");
    replacement.shutdown();
    wait_for_port_release(port).await;
}
