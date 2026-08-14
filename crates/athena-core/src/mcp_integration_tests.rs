//! Public MCP server lifecycle and resource-budget integration tests.

use super::McpServer;
use crate::agent_comms::AgentComms;
use crate::output_buffer::OutputBuffer;
use crate::plan_manager::{ExecutionPlan, PlanManager};
use crate::tool_executor::{ToolEventSender, ToolExecutor};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

struct NoopEventSender;

impl ToolEventSender for NoopEventSender {
    fn agent_spawned(&self, _id: &str, _agent_type: &str, _agent_cmd: &str) {}
    fn close_panes(&self, _pane_ids: &[String]) {}
    fn pty_write(&self, _pane_id: &str, _data: &str) {}
    fn has_session(&self, _pane_id: &str) -> bool {
        false
    }
    fn ask_user(
        &self,
        _request_id: &str,
        _question: &str,
        _options: &[serde_json::Value],
    ) -> String {
        String::new()
    }
    fn plan_update(&self, _plan: &ExecutionPlan) {}
    fn plan_evaluated(
        &self,
        _plan_id: &str,
        _overall_status: &str,
        _step_evaluations: &[serde_json::Value],
        _next_action: &str,
        _reasoning: &str,
    ) {
    }
}

fn test_tool_executor() -> std::sync::Arc<parking_lot::Mutex<ToolExecutor>> {
    std::sync::Arc::new(parking_lot::Mutex::new(ToolExecutor::new(
        std::sync::Arc::new(OutputBuffer::new()),
        std::sync::Arc::new(PlanManager::new()),
        std::sync::Arc::new(AgentComms::new()),
        std::sync::Arc::new(NoopEventSender),
        std::sync::Arc::new(athena_store::KeyValueStore::new_empty()),
        None,
    )))
}

async fn read_line_with_timeout<R>(reader: &mut R) -> Result<Option<String>, &'static str>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    let bytes = tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .map_err(|_| "timed out waiting for MCP response")?
        .map_err(|_| "failed to read MCP response")?;
    Ok((bytes > 0).then_some(line))
}

#[tokio::test(flavor = "current_thread")]
async fn tcp_server_exposes_connection_budget_and_releases_port() {
    let app_shutdown = Arc::new(AtomicBool::new(false));
    let mut server = McpServer::new_with_shutdown(app_shutdown);
    server.init(0).expect("ephemeral MCP port should bind");
    let port = server.port().expect("server should publish a port");
    assert!(port > 0);

    server.request_shutdown();
    assert!(server.wait_for_tcp_shutdown().await);
    server.shutdown();
    assert!(server.port().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn tcp_tools_call_uses_the_configured_tool_executor() {
    let app_shutdown = Arc::new(AtomicBool::new(false));
    let mut server = McpServer::new_with_shutdown(app_shutdown);
    server.tool_executor = Some(test_tool_executor());
    let token = server.get_token().to_string();
    server.init(0).expect("ephemeral MCP port should bind");
    let port = server.port().expect("server should publish a port");

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("MCP TCP connection should succeed");
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(read_half);

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "token": token }
    });
    writer
        .write_all(format!("{}\n", initialize).as_bytes())
        .await
        .expect("initialize should be written");
    let _ = read_line_with_timeout(&mut reader)
        .await
        .expect("initialize should receive a response");

    let list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    writer
        .write_all(format!("{}\n", list).as_bytes())
        .await
        .expect("tool list should be written");
    let line = read_line_with_timeout(&mut reader)
        .await
        .expect("tool list should receive a response")
        .expect("tool list should receive a response line");
    let tool_list: serde_json::Value =
        serde_json::from_str(line.trim()).expect("tool list response must be valid JSON");
    let names: Vec<_> = tool_list["result"]["tools"]
        .as_array()
        .expect("tools should be an array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"list_agent_panes"));
    assert!(names.contains(&"fs_read_file"));
    assert!(names.contains(&"kanban_create_task"));
    assert!(!names.contains(&"request_input"));

    let create_task = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "create_tasks",
            "arguments": {
                "spaceId": "space-1",
                "tasks": [{ "title": "TCP task" }]
            }
        }
    });
    writer
        .write_all(format!("{}\n", create_task).as_bytes())
        .await
        .expect("task call should be written");
    let line = read_line_with_timeout(&mut reader)
        .await
        .expect("task call should receive a response")
        .expect("task call should receive a response line");
    let task_response: serde_json::Value =
        serde_json::from_str(line.trim()).expect("task response must be valid JSON");
    assert_eq!(task_response["id"], 3);
    assert!(task_response["result"]["content"][0]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Task created: TCP task")));

    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "list_agents", "arguments": {} }
    });
    writer
        .write_all(format!("{}\n", call).as_bytes())
        .await
        .expect("tool call should be written");
    let line = read_line_with_timeout(&mut reader)
        .await
        .expect("executor-backed tool should receive a response")
        .expect("executor-backed tool should receive a response line");
    let response: serde_json::Value =
        serde_json::from_str(line.trim()).expect("tool response must be valid JSON");
    assert_eq!(response["id"], 4);
    assert_eq!(response["error"], serde_json::Value::Null);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "No agents currently running."
    );

    server.request_shutdown();
    assert!(server.wait_for_tcp_shutdown().await);
    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn stdio_loop_uses_the_configured_tool_executor() {
    let (client, server) = tokio::io::duplex(16 * 1024);
    let (server_read, server_write) = tokio::io::split(server);
    let server_task = tokio::spawn(super::run_stdio_loop(
        tokio::io::BufReader::new(server_read),
        server_write,
        "stdio-test-token".to_string(),
        None,
        None,
        None,
        None,
        Some(test_tool_executor()),
    ));

    let (client_read, mut client_write) = tokio::io::split(client);
    let mut client_reader = tokio::io::BufReader::new(client_read);
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "list_agents", "arguments": {} }
    });
    client_write
        .write_all(format!("{}\n", request).as_bytes())
        .await
        .expect("stdio request should be written");

    let line = read_line_with_timeout(&mut client_reader)
        .await
        .expect("stdio response should arrive")
        .expect("stdio response should include a line");
    let response: serde_json::Value =
        serde_json::from_str(line.trim()).expect("stdio response must be valid JSON");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "No agents currently running."
    );

    // Close the client side of the duplex to signal stdin EOF to the stdio
    // loop. `tokio::io::split` keeps both halves alive behind a shared `Arc`,
    // so dropping the write half alone does not propagate EOF — the read half
    // must be dropped too (dropping both halves drops the whole duplex end).
    drop(client_write);
    drop(client_reader);
    assert!(server_task
        .await
        .expect("stdio task should not panic")
        .is_ok());
}

#[test]
fn legacy_mcp_arguments_are_normalized_for_the_executor() {
    let executor = test_tool_executor();

    let create_tasks = serde_json::json!({
        "spaceId": "space-1",
        "tasks": [{ "title": "External task", "description": "Created over MCP" }]
    });
    let created = super::execute_mcp_tool_call(&executor.lock(), "create_tasks", &create_tasks)
        .expect("create_tasks should normalize to kanban_create_task");
    assert!(created.text.contains("Task created: External task"));

    let spawn_agents = serde_json::json!({
        "count": 2,
        "instruction": "Run the external verification"
    });
    let spawned = super::execute_mcp_tool_call(&executor.lock(), "spawn_agents", &spawn_agents)
        .expect("spawn_agents should normalize count/instruction");
    assert!(spawned.text.contains("launched 2 claude agents"));
}

#[test]
fn external_discovery_excludes_placeholder_only_tools() {
    let names: Vec<_> = super::get_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert!(names.contains(&"create_tasks".to_string()));
    assert!(names.contains(&"spawn_agents".to_string()));
    assert!(names.contains(&"check_agent_status".to_string()));
    assert!(names.contains(&"workspace_switch".to_string()));
    assert!(!names.contains(&"request_input".to_string()));
    assert!(!names.contains(&"send_message_to_agent".to_string()));

    // Discovery and routing must be generated from the same canonical set;
    // otherwise clients receive tools that the transport silently rejects.
    for name in names {
        assert!(
            super::is_executor_mcp_tool(&name),
            "discovered tool {name} must be executor-routable"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_tcp_json_returns_a_parse_error_without_panicking() {
    let mut server = McpServer::new();
    server.init(0).expect("ephemeral MCP port should bind");
    let port = server.port().expect("server should publish a port");

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("MCP TCP connection should succeed");
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(read_half);

    // The id is recoverable even though the JSON object is truncated.
    writer
        .write_all(b"{\"id\":42,\"method\":\"tools/call\"")
        .await
        .expect("malformed request should be accepted for parsing");
    writer.write_all(b"\n").await.unwrap();

    let line = read_line_with_timeout(&mut reader)
        .await
        .expect("malformed JSON should receive a response")
        .expect("malformed JSON should receive a response line");
    let response: serde_json::Value =
        serde_json::from_str(line.trim()).expect("parse error response must itself be valid JSON");
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 42);
    assert_eq!(response["error"]["code"], -32700);

    server.request_shutdown();
    assert!(server.wait_for_tcp_shutdown().await);
    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_tcp_json_line_is_rejected_and_connection_closes() {
    let mut server = McpServer::new();
    server.init(0).expect("ephemeral MCP port should bind");
    let port = server.port().expect("server should publish a port");

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("MCP TCP connection should succeed");
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(read_half);

    // Exceed the transport's 1 MiB line cap without relying on a private
    // implementation constant in this integration test.
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    writer
        .write_all(&oversized)
        .await
        .expect("oversized payload should reach the transport");
    writer.write_all(b"\n").await.unwrap();

    // Oversized input is deliberately dropped without a JSON-RPC response.
    assert_eq!(
        read_line_with_timeout(&mut reader)
            .await
            .expect("oversized input should close the connection before the read timeout"),
        None,
        "oversized MCP line should close the connection without a response"
    );
    server.request_shutdown();
    assert!(server.wait_for_tcp_shutdown().await);
    server.shutdown();
}
