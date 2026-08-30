//! Integration tests: plugin manifest security boundaries and manager
//! capability isolation.
//!
//! Contracts under defense:
//! - manifest validation: MCP commands must be bare whitelisted executable
//!   names; MCP env must not override PATH/HOME; oversized config JSON
//!   (>256 KiB) rejected; ids bounded to 128 bytes of [A-Za-z0-9._-];
//! - discovery boundaries: oversized manifest files (>1 MiB) are silently
//!   skipped, bad JSON surfaces as ManifestParse errors, valid manifests
//!   parse, non-JSON files are ignored;
//! - capability isolation: a session's effective capabilities never exceed
//!   what its plugin manifest declares, regardless of what it requests;
//! - event isolation: subscriptions only route to the subscribing session;
//! - oversized event payloads are replaced with an error payload;
//! - message ownership: relaying into another plugin's session is rejected;
//! - oversized message params (>256 KiB) rejected;
//! - MAX_PLUGIN_SESSIONS cap enforced (257th session fails).

use std::collections::HashMap;

use athena_plugins::{
    AgentType, McpConfig, PluginCapability, PluginError, PluginEventSource, PluginEventType,
    PluginInstallMethod, PluginManager, PluginManifest,
};

fn minimal_manifest(id: &str) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "integration fixture".to_string(),
        author: "tests".to_string(),
        permissions: Vec::new(),
        mcp_config: None,
        min_athena_version: None,
        capabilities: Vec::new(),
        tools: Vec::new(),
        subscribes_to: None,
        config: None,
        install: Some(PluginInstallMethod::Builtin),
    }
}

fn mcp_manifest(id: &str, command: &str, env: Option<HashMap<String, String>>) -> PluginManifest {
    let mut manifest = minimal_manifest(id);
    manifest.mcp_config = Some(McpConfig {
        command: command.to_string(),
        args: vec![],
        env,
    });
    manifest
}

#[test]
fn mcp_command_must_be_bare_whitelisted_executable() {
    let manager = PluginManager::new();

    for command in [
        "/usr/bin/node",
        "./node",
        "../node",
        "bin/node",
        "curl",
        "evil-binary",
    ] {
        let err = manager
            .register_plugin(mcp_manifest("mcp-cmd", command, None))
            .unwrap_err();
        assert!(
            matches!(err, PluginError::ValidationFailed(_)),
            "command {command:?} must be rejected, got {err:?}"
        );
    }

    // Whitelisted bare name accepted.
    manager
        .register_plugin(mcp_manifest("mcp-ok", "node", None))
        .expect("bare whitelisted command accepted");
}

#[test]
fn mcp_env_cannot_override_path_or_home() {
    let manager = PluginManager::new();

    let mut env = HashMap::new();
    env.insert("PATH".to_string(), "/tmp/evil".to_string());
    let err = manager
        .register_plugin(mcp_manifest("env-path", "node", Some(env)))
        .unwrap_err();
    assert!(matches!(err, PluginError::ValidationFailed(_)));

    let mut env = HashMap::new();
    env.insert("HOME".to_string(), "/tmp/evil".to_string());
    let err = manager
        .register_plugin(mcp_manifest("env-home", "node", Some(env)))
        .unwrap_err();
    assert!(matches!(err, PluginError::ValidationFailed(_)));

    // Benign env accepted.
    let mut env = HashMap::new();
    env.insert("LOG_LEVEL".to_string(), "debug".to_string());
    manager
        .register_plugin(mcp_manifest("env-ok", "node", Some(env)))
        .expect("benign env accepted");
}

#[test]
fn identifier_and_config_size_boundaries() {
    let manager = PluginManager::new();

    // Empty id and >128-byte id rejected.
    let mut manifest = minimal_manifest("");
    assert!(manager.register_plugin(manifest.clone()).is_err());
    manifest.id = "x".repeat(129);
    assert!(manager.register_plugin(manifest).is_err());

    // Disallowed identifier characters rejected.
    let mut manifest = minimal_manifest("bad/id");
    assert!(manager.register_plugin(manifest.clone()).is_err());
    manifest.id = "bad id".into();
    assert!(manager.register_plugin(manifest).is_err());

    // Oversized config JSON (>256 KiB) rejected.
    let mut manifest = minimal_manifest("big-config");
    manifest.config = Some(athena_plugins::PluginConfigSchema {
        schema: serde_json::json!({ "pad": "z".repeat(256 * 1024 + 1) }),
        defaults: serde_json::Value::Null,
    });
    let err = manager.register_plugin(manifest).unwrap_err();
    assert!(matches!(err, PluginError::LimitExceeded(_)));
}

#[test]
fn discovery_skips_oversized_and_reports_bad_json() {
    let dir = tempfile::tempdir().unwrap();

    // Oversized manifest: silently skipped (not even an Err entry).
    let oversized = dir.path().join("oversized.json");
    std::fs::write(&oversized, vec![b' '; 1_048_576 + 1]).unwrap();

    // Bad JSON: ManifestParse error entry.
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "{ not json").unwrap();

    // Valid manifest: parses and validates.
    let good = dir.path().join("good.json");
    let manifest = minimal_manifest("discovered-plugin");
    std::fs::write(&good, serde_json::to_string(&manifest).unwrap()).unwrap();

    // Valid manifest that fails security validation: ValidationFailed entry.
    let insecure = dir.path().join("insecure.json");
    let mut bad_manifest = minimal_manifest("insecure-plugin");
    bad_manifest.mcp_config = Some(McpConfig {
        command: "/bin/evil".to_string(),
        args: vec![],
        env: None,
    });
    std::fs::write(&insecure, serde_json::to_string(&bad_manifest).unwrap()).unwrap();

    // Non-JSON file: ignored entirely.
    std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();

    let manager = PluginManager::new();
    let results = manager.discover_plugins(dir.path()).unwrap();

    let parse_errors = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .filter(|e| matches!(e, PluginError::ManifestParse { .. }))
        .count();
    let validation_errors = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .filter(|e| matches!(e, PluginError::ValidationFailed(_)))
        .count();
    let ok = results.iter().filter(|r| r.is_ok()).count();

    assert_eq!(ok, 1, "only the valid manifest parses");
    assert_eq!(parse_errors, 1, "bad JSON surfaces a parse error");
    assert_eq!(
        validation_errors, 1,
        "insecure manifest surfaces validation failure"
    );
    assert_eq!(
        results.len(),
        3,
        "oversized file silently skipped, non-JSON ignored"
    );
}

#[test]
fn session_capabilities_cannot_exceed_manifest_declaration() {
    let manager = PluginManager::new();

    // Plugin declares ONLY Notifications.
    let mut manifest = minimal_manifest("limited");
    manifest.capabilities = vec![PluginCapability::Notifications];
    manager.register_plugin(manifest).unwrap();

    // Session requests Notifications + FileAccess + Swarm.
    let session = manager
        .register_session(
            "limited",
            AgentType::Claude,
            None,
            None,
            Some(vec![
                PluginCapability::Notifications,
                PluginCapability::FileAccess,
                PluginCapability::Swarm,
            ]),
        )
        .expect("session registered");

    assert_eq!(
        session.capabilities,
        vec![PluginCapability::Notifications],
        "requested capabilities clipped to the manifest declaration"
    );
}

#[test]
fn event_subscription_routes_only_to_subscriber_and_enforces_declaration() {
    let manager = PluginManager::new();

    let mut manifest_a = minimal_manifest("plugin-a");
    manifest_a.subscribes_to = Some(vec![PluginEventType::Notification]);
    manager.register_plugin(manifest_a).unwrap();
    manager
        .register_plugin(minimal_manifest("plugin-b"))
        .unwrap();

    let session_a = manager
        .register_session("plugin-a", AgentType::Claude, None, None, None)
        .unwrap();
    let session_b = manager
        .register_session("plugin-b", AgentType::Claude, None, None, None)
        .unwrap();

    // Undeclared event type rejected for plugin-a.
    let undeclared = manager
        .subscribe_session_owned("plugin-a", &session_a.id, &[PluginEventType::AgentSpawned])
        .unwrap_err();
    assert!(matches!(undeclared, PluginError::ValidationFailed(_)));

    // Ownership: session_b cannot subscribe under plugin-a's name.
    let ownership = manager
        .subscribe_session_owned("plugin-a", &session_b.id, &[PluginEventType::Notification])
        .unwrap_err();
    assert!(matches!(ownership, PluginError::SessionOwnership { .. }));

    // Declared subscription accepted.
    manager
        .subscribe_session_owned("plugin-a", &session_a.id, &[PluginEventType::Notification])
        .expect("declared subscription accepted");

    // Routing: only session_a appears as a Notification subscriber.
    let subscribers = manager.get_subscribers(&PluginEventType::Notification);
    assert_eq!(subscribers.len(), 1);
    assert_eq!(subscribers[0].id, session_a.id);
    assert!(manager
        .get_subscribers(&PluginEventType::TaskComplete)
        .is_empty());
}

#[test]
fn oversized_event_payload_is_replaced_with_error_payload() {
    let manager = PluginManager::new();
    let oversized_payload = athena_plugins::PluginEventPayload {
        level: None,
        message: Some("y".repeat(256 * 1024 + 1)),
        title: None,
        metadata: None,
        task_title: None,
        result: None,
        error: None,
        prompt: None,
        options: None,
        request_id: None,
        response: None,
        exit_code: None,
        command: None,
        session_id: None,
        agent_id: None,
        plugin_id: None,
    };
    let event = manager.emit_plugin_event(
        PluginEventType::ProgressUpdate,
        PluginEventSource {
            session_id: "test-session".into(),
            pane_id: None,
            agent_type: "claude".into(),
            agent_id: None,
        },
        oversized_payload,
    );
    assert_eq!(
        event.payload.message.as_deref(),
        Some("plugin event payload exceeded size limit"),
        "oversized payload must be replaced, not forwarded"
    );
    assert!(matches!(
        event.payload.level,
        Some(athena_plugins::PayloadLevel::Error)
    ));
}

#[test]
fn message_relay_enforces_session_ownership_and_size_caps() {
    let manager = PluginManager::new();
    manager.register_plugin(minimal_manifest("owner")).unwrap();
    manager.register_plugin(minimal_manifest("other")).unwrap();

    let session = manager
        .register_session("owner", AgentType::Claude, None, None, None)
        .unwrap();
    let foreign = manager
        .register_session("other", AgentType::Claude, None, None, None)
        .unwrap();

    // Relay into another plugin's session rejected.
    let err = manager
        .send_message_owned("owner", &foreign.id, "run", serde_json::json!({}))
        .unwrap_err();
    assert!(matches!(err, PluginError::SessionOwnership { .. }));

    // Oversized params rejected.
    let err = manager
        .send_message_owned(
            "owner",
            &session.id,
            "run",
            serde_json::json!({ "blob": "x".repeat(256 * 1024 + 1) }),
        )
        .unwrap_err();
    assert!(matches!(err, PluginError::LimitExceeded(_)));

    // In-bounds relay queues a pending message.
    let pending = manager
        .send_message_owned("owner", &session.id, "run", serde_json::json!({ "n": 1 }))
        .expect("in-bounds relay accepted");
    assert_eq!(pending.method, "run");
}

#[test]
fn session_cap_rejects_the_257th_session() {
    let manager = PluginManager::new();
    manager.register_plugin(minimal_manifest("farm")).unwrap();

    // MAX_PLUGIN_SESSIONS = 256; the first 256 must succeed.
    for i in 0..256 {
        manager
            .register_session(
                "farm",
                AgentType::Claude,
                Some(format!("agent-{i}")),
                None,
                None,
            )
            .unwrap_or_else(|e| panic!("session {i} should register: {e:?}"));
    }
    let err = manager
        .register_session(
            "farm",
            AgentType::Claude,
            Some("agent-256".into()),
            None,
            None,
        )
        .unwrap_err();
    assert!(matches!(err, PluginError::LimitExceeded(_)));
}
