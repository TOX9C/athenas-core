//! Integration tests: swarm coordinator persistence, validation, and
//! file-backed mailboxes.
//!
//! Contracts under defense:
//! - state persists to `<dir>/.ade/swarm-state.json` atomically and reads
//!   back identically (revision bumped, workspace_dir stamped);
//! - identifier validation: empty/`.`/`..`/path separators/control chars and
//!   ids >128 bytes are rejected; oversized content (>64 KiB) rejected;
//! - lifecycle transitions validate status enums (agent + task + mission);
//! - revision bumps on every mutation; stale readers see the new revision;
//! - task lifecycle queued→done sets completed_at;
//! - send_message requires BOTH sender and receiver to be registered agents
//!   and delivers to the receiver's mailbox file (flock-serialized);
//! - concurrent sends from multiple tasks do not lose messages;
//! - a corrupted mailbox file is quarantined to `.corrupt` rather than
//!   poisoning future sends;
//! - read_state on a directory without state returns None.

use athena_core::swarm::{MailboxMessage, SwarmAgent, SwarmCoordinator, SwarmError, SwarmState};

fn temp_dir(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "athena-swarm-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn mission(id: &str, agent_ids: &[&str]) -> SwarmState {
    SwarmState {
        id: id.to_string(),
        goal: "ship it".into(),
        agents: agent_ids
            .iter()
            .map(|aid| SwarmAgent {
                id: aid.to_string(),
                pane_id: format!("pane-{aid}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[tokio::test]
async fn create_swarm_persists_and_read_back_is_identical() {
    let dir = temp_dir("create");
    let coord = SwarmCoordinator::new();
    let created = coord
        .create_swarm(&dir, mission("m-1", &["alpha", "beta"]))
        .await
        .expect("create");
    assert_eq!(created.revision, 1, "creation bumps revision to >=1");
    assert_eq!(created.workspace_dir, dir);
    assert_eq!(created.status, "active");

    let loaded = coord.read_state(&dir).await.unwrap().expect("state exists");
    assert_eq!(loaded, created, "round-trip must be lossless");

    // No state in a fresh directory.
    let empty = coord.read_state(&temp_dir("none")).await.unwrap();
    assert!(empty.is_none());
}

#[tokio::test]
async fn invalid_identifiers_and_content_are_rejected() {
    let dir = temp_dir("validate");
    let coord = SwarmCoordinator::new();

    let bad_ids = ["", ".", "..", "a/b", "a\\b", "a\nb"];
    for id in bad_ids {
        let mut state = mission("m-1", &["alpha"]);
        state.id = id.to_string();
        let err = coord.create_swarm(&dir, state).await.unwrap_err();
        assert!(matches!(err, SwarmError::InvalidIdentifier(_)), "id {id:?}");
    }
    // 129-byte id exceeds MAX_ID_BYTES.
    let mut state = mission(&"x".repeat(129), &["alpha"]);
    let err = coord.create_swarm(&dir, state.clone()).await.unwrap_err();
    assert!(matches!(err, SwarmError::InvalidIdentifier(_)));

    // 64 KiB + 1 goal exceeds MAX_CONTENT_BYTES.
    state.id = "m-big".into();
    state.goal = "y".repeat(64 * 1024 + 1);
    let err = coord.create_swarm(&dir, state).await.unwrap_err();
    assert!(matches!(err, SwarmError::ContentTooLarge(limit) if limit == 64 * 1024));
}

#[tokio::test]
async fn agent_status_enum_is_enforced() {
    let dir = temp_dir("status");
    let coord = SwarmCoordinator::new();
    coord
        .create_swarm(&dir, mission("m-1", &["alpha"]))
        .await
        .unwrap();

    coord
        .update_agent(&dir, "alpha", Some("thinking".into()), None, None)
        .await
        .expect("valid status");
    let err = coord
        .update_agent(&dir, "alpha", Some("vibrating".into()), None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, SwarmError::InvalidIdentifier(_)));

    // Unknown agent rejected even with a valid status.
    let err = coord
        .update_agent(&dir, "ghost", Some("idle".into()), None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, SwarmError::AgentNotFound(id) if id == "ghost"));
}

#[tokio::test]
async fn mutations_bump_revision_and_task_lifecycle_sets_completed_at() {
    let dir = temp_dir("revision");
    let coord = SwarmCoordinator::new();
    let s0 = coord
        .create_swarm(&dir, mission("m-1", &["alpha"]))
        .await
        .unwrap();
    let s1 = coord
        .create_task(&dir, "write tests".into(), "unit".into(), "alpha".into())
        .await
        .expect("task created for registered agent");
    assert_eq!(s1.revision, s0.revision + 1);
    let task_id = s1.tasks[0].id.clone();
    assert_eq!(s1.tasks[0].status, "queued");
    assert!(s1.tasks[0].completed_at.is_none());

    // Task for an unregistered agent is rejected.
    let err = coord
        .create_task(&dir, "t".into(), "d".into(), "ghost".into())
        .await
        .unwrap_err();
    assert!(matches!(err, SwarmError::AgentNotFound(_)));

    let s2 = coord.update_task(&dir, &task_id, "done").await.unwrap();
    assert_eq!(s2.revision, s1.revision + 1);
    assert!(
        s2.tasks[0].completed_at.is_some(),
        "done stamps completed_at"
    );

    // Invalid task status rejected; unknown task id rejected.
    assert!(matches!(
        coord.update_task(&dir, &task_id, "archived").await,
        Err(SwarmError::InvalidIdentifier(_))
    ));
    assert!(matches!(
        coord.update_task(&dir, "task-nope", "done").await,
        Err(SwarmError::TaskNotFound(_))
    ));

    // Mission-level status enum.
    let paused = coord.set_status(&dir, "paused").await.unwrap();
    assert_eq!(paused.status, "paused");
    assert!(matches!(
        coord.set_status(&dir, "warp").await,
        Err(SwarmError::InvalidIdentifier(_))
    ));
}

#[tokio::test]
async fn send_message_requires_registered_sender_and_receiver() {
    let dir = temp_dir("mailauth");
    let coord = SwarmCoordinator::new();
    coord
        .create_swarm(&dir, mission("m-1", &["alpha", "beta"]))
        .await
        .unwrap();

    coord
        .send_message(&dir, "alpha", "beta", "hello beta")
        .await
        .expect("both registered");

    for (from, to) in [("ghost", "beta"), ("alpha", "ghost")] {
        let err = coord.send_message(&dir, from, to, "hi").await.unwrap_err();
        assert!(matches!(err, SwarmError::AgentNotFound(_)), "{from}->{to}");
    }

    let mailbox = coord.read_mailbox(&dir, "beta").await.unwrap();
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox[0].from, "alpha");
    assert_eq!(mailbox[0].content, "hello beta");
    assert!(!mailbox[0].read);

    // Sender has an empty mailbox; unknown reader rejected.
    assert!(coord.read_mailbox(&dir, "alpha").await.unwrap().is_empty());
    assert!(matches!(
        coord.read_mailbox(&dir, "ghost").await,
        Err(SwarmError::AgentNotFound(_))
    ));

    // Mission feed mirrors the mailbox message.
    let state = coord.read_state(&dir).await.unwrap().unwrap();
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].content, "hello beta");
}

#[tokio::test]
async fn concurrent_sends_do_not_lose_messages() {
    let dir = temp_dir("conc");
    let coord = SwarmCoordinator::new();
    coord
        .create_swarm(&dir, mission("m-1", &["alpha", "beta"]))
        .await
        .unwrap();

    // 5 concurrent senders → 5 delivered messages (flock serializes writers).
    let mut senders = Vec::new();
    for i in 0..5 {
        let coord = coord.clone();
        let dir = dir.clone();
        senders.push(tokio::spawn(async move {
            coord
                .send_message(&dir, "alpha", "beta", &format!("msg-{i}"))
                .await
        }));
    }
    for handle in senders {
        handle.await.unwrap().expect("send ok");
    }
    let mailbox = coord.read_mailbox(&dir, "beta").await.unwrap();
    let mut contents: Vec<&str> = mailbox.iter().map(|m| m.content.as_str()).collect();
    contents.sort();
    assert_eq!(
        contents,
        vec!["msg-0", "msg-1", "msg-2", "msg-3", "msg-4"],
        "flock must prevent lost updates"
    );
}

#[tokio::test]
async fn corrupted_mailbox_is_quarantined_not_fatal() {
    let dir = temp_dir("corrupt");
    let coord = SwarmCoordinator::new();
    coord
        .create_swarm(&dir, mission("m-1", &["alpha", "beta"]))
        .await
        .unwrap();

    let mailbox_path = std::path::Path::new(&dir)
        .join(".ade")
        .join("mailbox")
        .join("beta.json");
    std::fs::create_dir_all(mailbox_path.parent().unwrap()).unwrap();
    std::fs::write(&mailbox_path, "{not json").unwrap();

    coord
        .send_message(&dir, "alpha", "beta", "after corruption")
        .await
        .expect("corrupt mailbox quarantined");

    let mailbox = coord.read_mailbox(&dir, "beta").await.unwrap();
    assert_eq!(mailbox.len(), 1, "garbage replaced by fresh message");
    assert_eq!(mailbox[0].content, "after corruption");

    let sidecar = mailbox_path.with_extension("json.corrupt");
    // rename() keeps the full filename: beta.json → beta.json.corrupt
    let renamed = std::path::Path::new(&dir)
        .join(".ade")
        .join("mailbox")
        .join("beta.json.corrupt");
    let _ = sidecar;
    assert!(renamed.exists(), "quarantine sidecar must exist");

    // Oversized payload rejected before touching the mailbox.
    let err = coord
        .send_message(&dir, "alpha", "beta", &"z".repeat(64 * 1024 + 1))
        .await
        .unwrap_err();
    assert!(matches!(err, SwarmError::ContentTooLarge(_)));
}

#[tokio::test]
async fn mailbox_message_serialization_round_trip() {
    let msg = MailboxMessage {
        id: "msg-1".into(),
        from: "a".into(),
        to: "b".into(),
        content: "hello".into(),
        timestamp: 1_700_000_000_000,
        read: false,
    };
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["from"], "a", "camelCase rename on the wire");
    let back: MailboxMessage = serde_json::from_value(value).unwrap();
    assert_eq!(back, msg);
}
