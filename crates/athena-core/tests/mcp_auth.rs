//! Integration test: MCP TCP connections require `initialize` before any other method.

use athena_core::mcp::{JsonRpcRequest, JsonRpcResponse, McpServer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

fn jsonrpc_id(id: u64) -> Option<serde_json::Value> {
    Some(serde_json::Value::Number(serde_json::Number::from(id)))
}

async fn read_response<R>(reader: &mut R) -> JsonRpcResponse
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let trimmed = line.trim();
    serde_json::from_str(trimmed).unwrap()
}

#[tokio::test]
async fn mcp_requires_initialize_before_tool_call() {
    let mut server = McpServer::new();
    let token = server.get_token().to_string();
    server.init(0).expect("Failed to init MCP server");

    let port = server.port().expect("Server should have a port");

    // --- Connection 1: unauthenticated tool call is rejected ---
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("Failed to connect to MCP server");
    let (read_half, write_half) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(read_half);
    let mut writer = write_half;

    // tools/call without initialize
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: jsonrpc_id(1),
        method: "tools/call".into(),
        params: serde_json::json!({
            "name": "notify",
            "arguments": { "message": "hello" }
        }),
    };
    let line = serde_json::to_string(&req).unwrap() + "\n";
    writer.write_all(line.as_bytes()).await.unwrap();

    let response = read_response(&mut reader).await;
    assert!(
        response.result.is_none(),
        "tools/call without init should fail"
    );
    let err = response.error.expect("Expected error response");
    assert_eq!(err.code, -32600, "Expected error -32600 (invalid request)");
    assert!(err.message.contains("Unauthenticated"));

    // --- Initialize connection 1 ---
    let init_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: jsonrpc_id(2),
        method: "initialize".into(),
        params: serde_json::json!({ "token": token }),
    };
    let line = serde_json::to_string(&init_req).unwrap() + "\n";
    writer.write_all(line.as_bytes()).await.unwrap();

    let init_response = read_response(&mut reader).await;
    assert!(
        init_response.error.is_none(),
        "Valid initialize should succeed"
    );
    assert!(
        init_response.result.is_some(),
        "initialize should return a result"
    );

    // --- tools/call after init on connection 1 should succeed ---
    let req2 = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: jsonrpc_id(3),
        method: "tools/call".into(),
        params: serde_json::json!({
            "name": "notify",
            "arguments": { "message": "hello again" }
        }),
    };
    let line = serde_json::to_string(&req2).unwrap() + "\n";
    writer.write_all(line.as_bytes()).await.unwrap();

    let resp2 = read_response(&mut reader).await;
    assert!(
        resp2.error.is_none(),
        "tools/call after init should succeed: {:?}",
        resp2.error
    );

    // --- Connection 2: should NOT inherit auth from connection 1 ---
    let stream2 = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("Failed to connect to MCP server (2nd connection)");
    let (read2, write2) = tokio::io::split(stream2);
    let mut reader2 = tokio::io::BufReader::new(read2);
    let mut writer2 = write2;

    let list_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: jsonrpc_id(4),
        method: "tools/list".into(),
        params: serde_json::Value::Null,
    };
    let line = serde_json::to_string(&list_req).unwrap() + "\n";
    writer2.write_all(line.as_bytes()).await.unwrap();

    let list_response = read_response(&mut reader2).await;
    assert!(
        list_response.result.is_none(),
        "tools/list without init on new connection should fail"
    );
    let err = list_response
        .error
        .expect("Expected error on new connection");
    assert_eq!(err.code, -32600, "Expected error -32600 on new connection");
    assert!(err.message.contains("Unauthenticated"));

    server.shutdown();
}
