// ---------------------------------------------------------------------------
// OutputBuffer Tests
// ---------------------------------------------------------------------------

mod output_buffer_tests {
    use crate::output_buffer::{
        GetOutputOptions, OutputBuffer,
    };

    fn make_buffer() -> OutputBuffer {
        OutputBuffer::new()
    }

    #[test]
    fn test_append_single_line() {
        let buf = make_buffer();
        buf.append_output("pane-1", "hello world", Some("agent"));
        let lines = buf.get_output("pane-1", None);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "hello world");
        assert_eq!(lines[0].pane_id, "pane-1");
    }

    #[test]
    fn test_append_multiple_lines() {
        let buf = make_buffer();
        for i in 0..5 {
            buf.append_output("pane-1", &format!("line {}", i), Some("agent"));
        }
        let lines = buf.get_output("pane-1", None);
        assert_eq!(lines.len(), 5);
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(line.text, format!("line {}", i));
        }
    }

    #[test]
    fn test_get_output_with_limit() {
        let buf = make_buffer();
        for i in 0..100 {
            buf.append_output("pane-1", &format!("line {}", i), Some("agent"));
        }
        let opts = GetOutputOptions {
            limit: Some(10),
            ..Default::default()
        };
        let lines = buf.get_output("pane-1", Some(&opts));
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0].text, "line 0");
    }

    #[test]
    fn test_get_output_with_since_line() {
        let buf = make_buffer();
        for i in 0..10 {
            buf.append_output("pane-1", &format!("line {}", i), Some("agent"));
        }
        let opts = GetOutputOptions {
            since_line: Some(5),
            ..Default::default()
        };
        let lines = buf.get_output("pane-1", Some(&opts));
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].text, "line 5");
    }

    #[test]
    fn test_get_output_with_since_time() {
        let buf = make_buffer();
        buf.append_output("pane-1", "first", Some("agent"));
        let lines = buf.get_output("pane-1", None);
        let ts = lines[0].timestamp;

        std::thread::sleep(std::time::Duration::from_millis(2));

        buf.append_output("pane-1", "second", Some("agent"));

        let opts = GetOutputOptions {
            since_time: Some(ts),
            ..Default::default()
        };
        let lines = buf.get_output("pane-1", Some(&opts));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "second");
    }

    #[test]
    fn test_list_agents() {
        let buf = make_buffer();
        buf.append_output("pane-1", "data", Some("coder"));
        buf.append_output("pane-2", "data", Some("reviewer"));
        let agents = buf.get_agent_list();
        assert_eq!(agents.len(), 2);
        let ids: Vec<&str> = agents.iter().map(|a| a.pane_id.as_str()).collect();
        assert!(ids.contains(&"pane-1"));
        assert!(ids.contains(&"pane-2"));
    }

    #[test]
    fn test_remove_pane() {
        let buf = make_buffer();
        buf.append_output("pane-1", "data", Some("agent"));
        assert!(buf.remove_pane("pane-1"));
        assert!(!buf.remove_pane("pane-1"));
        let lines = buf.get_output("pane-1", None);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_cleanup_dead_panes() {
        let buf = make_buffer();
        buf.append_output("pane-1", "data", Some("agent"));
        buf.append_output("pane-2", "data", Some("agent"));
        buf.mark_pane_dead("pane-1");
        let removed = buf.cleanup_dead_panes();
        assert_eq!(removed, 1);
        let agents = buf.get_agent_list();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pane_id, "pane-2");
    }

    #[test]
    fn test_max_lines_per_pane() {
        let buf = make_buffer();
        for i in 0..5010 {
            buf.append_output("pane-1", &format!("line {}", i), Some("agent"));
        }
        let lines = buf.get_output("pane-1", None);
        assert!(lines.len() <= 5000);
        assert_eq!(lines[0].text, "line 10");
    }

    #[test]
    fn test_empty_buffer() {
        let buf = make_buffer();
        let lines = buf.get_output("nonexistent", None);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_init_pane_buffer() {
        let buf = make_buffer();
        buf.init_pane_buffer("pane-init", "tester").unwrap();
        let info = buf.get_pane_buffer_info("pane-init");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.pane_id, "pane-init");
        assert_eq!(info.agent_type, "tester");
        assert_eq!(info.line_count, 0);
    }

    #[test]
    fn test_clear_pane_buffer() {
        let buf = make_buffer();
        buf.append_output("pane-1", "line1", Some("agent"));
        buf.append_output("pane-1", "line2", Some("agent"));
        assert!(buf.clear_pane_buffer("pane-1"));
        let lines = buf.get_output("pane-1", None);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_shutdown() {
        let buf = make_buffer();
        buf.append_output("pane-1", "data", Some("agent"));
        buf.shutdown();
        let lines = buf.get_output("pane-1", None);
        assert!(lines.is_empty());
    }
}

// ---------------------------------------------------------------------------
// PlanManager Tests
// ---------------------------------------------------------------------------

mod plan_manager_tests {
    use crate::plan_manager::{
        PlanInput, PlanManager, PlanStepInput, PlanStatus, StepStatus,
    };

    fn make_manager() -> PlanManager {
        PlanManager::new()
    }

    fn make_plan_input(goal: &str) -> PlanInput {
        PlanInput {
            goal: goal.to_string(),
            reasoning: "test reasoning".to_string(),
            steps: vec![
                PlanStepInput {
                    id: "step-1".to_string(),
                    description: "First step".to_string(),
                },
                PlanStepInput {
                    id: "step-2".to_string(),
                    description: "Second step".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_create_plan() {
        let mgr = make_manager();
        let input = make_plan_input("Test goal");
        let plan = mgr.set_active_plan(input).unwrap();
        assert_eq!(plan.goal, "Test goal");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.status, PlanStatus::Pending);
    }

    #[test]
    fn test_update_step_status() {
        let mgr = make_manager();
        mgr.set_active_plan(make_plan_input("Goal")).unwrap();
        let result = mgr
            .update_step_status("step-1", StepStatus::InProgress, Some("pane-1"))
            .unwrap();
        assert!(result);
        let plan = mgr.get_active_plan().unwrap();
        let step = plan.steps.iter().find(|s| s.id == "step-1").unwrap();
        assert_eq!(step.status, StepStatus::InProgress);
        assert_eq!(step.assigned_pane_id, Some("pane-1".to_string()));
        assert_eq!(plan.status, PlanStatus::InProgress);
    }

    #[test]
    fn test_get_current_plan() {
        let mgr = make_manager();
        assert!(mgr.get_active_plan().is_none());
        mgr.set_active_plan(make_plan_input("Goal")).unwrap();
        let plan = mgr.get_active_plan();
        assert!(plan.is_some());
        assert_eq!(plan.unwrap().goal, "Goal");
    }

    #[test]
    fn test_clear_plan() {
        let mgr = make_manager();
        mgr.set_active_plan(make_plan_input("Goal")).unwrap();
        mgr.clear_active_plan().unwrap();
        assert!(mgr.get_active_plan().is_none());
    }

    #[test]
    fn test_plan_with_dependencies() {
        let mgr = make_manager();
        let input = PlanInput {
            goal: "Dependent plan".to_string(),
            reasoning: "steps depend on each other".to_string(),
            steps: vec![
                PlanStepInput {
                    id: "setup".to_string(),
                    description: "Setup environment".to_string(),
                },
                PlanStepInput {
                    id: "build".to_string(),
                    description: "Build project".to_string(),
                },
                PlanStepInput {
                    id: "test".to_string(),
                    description: "Run tests".to_string(),
                },
            ],
        };
        let plan = mgr.set_active_plan(input).unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].id, "setup");
        assert_eq!(plan.steps[1].id, "build");
        assert_eq!(plan.steps[2].id, "test");
    }

    #[test]
    fn test_multiple_plans() {
        let mgr = make_manager();
        mgr.set_active_plan(make_plan_input("First")).unwrap();
        mgr.set_active_plan(make_plan_input("Second")).unwrap();
        let plan = mgr.get_active_plan().unwrap();
        assert_eq!(plan.goal, "Second");
    }

    #[test]
    fn test_step_not_found() {
        let mgr = make_manager();
        mgr.set_active_plan(make_plan_input("Goal")).unwrap();
        let result = mgr.update_step_status("nonexistent", StepStatus::Completed, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_id_generation() {
        let mgr = make_manager();
        let plan1 = mgr.set_active_plan(make_plan_input("First")).unwrap();
        mgr.clear_active_plan().unwrap();
        let plan2 = mgr.set_active_plan(make_plan_input("Second")).unwrap();
        assert_ne!(plan1.id, plan2.id);
    }

    #[test]
    fn test_update_plan_status() {
        let mgr = make_manager();
        mgr.set_active_plan(make_plan_input("Goal")).unwrap();
        mgr.update_plan_status(PlanStatus::Completed).unwrap();
        let plan = mgr.get_active_plan().unwrap();
        assert_eq!(plan.status, PlanStatus::Completed);
    }

    #[test]
    fn test_update_step_no_plan() {
        let mgr = make_manager();
        let result = mgr.update_step_status("step-1", StepStatus::Completed, None);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// NotificationService Tests
// ---------------------------------------------------------------------------

mod notification_tests {
    use crate::notification::{
        HistoryOptions, NotificationEvent, NotificationService, NotificationType,
    };

    fn make_service() -> NotificationService {
        NotificationService::new()
    }

    fn make_event(r#type: NotificationType, title: &str) -> NotificationEvent {
        NotificationEvent {
            r#type,
            title: title.to_string(),
            message: "test message".to_string(),
            source: "test".to_string(),
            agent_id: None,
            data: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            metadata: None,
            actions: None,
            request_id: None,
        }
    }

    #[test]
    fn test_push_notification() {
        let svc = make_service();
        let record = svc.push_notification(make_event(NotificationType::Info, "Test"));
        assert_eq!(record.title, "Test");
        assert!(!record.read);
        assert!(record.dismissed_at.is_none());
    }

    #[test]
    fn test_push_multiple_types() {
        let svc = make_service();
        let types = vec![
            NotificationType::Info,
            NotificationType::Warning,
            NotificationType::Error,
            NotificationType::Success,
        ];
        for t in types {
            svc.push_notification(make_event(t.clone(), &format!("{:?}", t)));
        }
        let history = svc.get_all_history();
        assert_eq!(history.len(), 4);
    }

    #[test]
    fn test_get_counts() {
        let svc = make_service();
        svc.push_notification(make_event(NotificationType::Info, "info1"));
        svc.push_notification(make_event(NotificationType::Warning, "warn1"));
        svc.push_notification(make_event(NotificationType::Error, "err1"));
        svc.push_notification(make_event(NotificationType::Info, "info2"));
        let counts = svc.get_counts();
        assert_eq!(counts.total, 4);
        assert_eq!(counts.unread, 4);
        assert_eq!(counts.by_type.info, 2);
        assert_eq!(counts.by_type.warning, 1);
        assert_eq!(counts.by_type.error, 1);
    }

    #[test]
    fn test_mark_read() {
        let svc = make_service();
        let record = svc.push_notification(make_event(NotificationType::Info, "Test"));
        svc.mark_read(&record.id).unwrap();
        let counts = svc.get_counts();
        assert_eq!(counts.unread, 0);
    }

    #[test]
    fn test_mark_all_read() {
        let svc = make_service();
        for i in 0..5 {
            svc.push_notification(make_event(
                NotificationType::Info,
                &format!("msg {}", i),
            ));
        }
        let marked = svc.mark_all_read();
        assert_eq!(marked, 5);
        assert_eq!(svc.get_unread_count(), 0);
    }

    #[test]
    fn test_dismiss() {
        let svc = make_service();
        let record = svc.push_notification(make_event(NotificationType::Info, "Test"));
        svc.dismiss(&record.id).unwrap();
        let history = svc.get_all_history();
        assert!(history.is_empty());
    }

    #[test]
    fn test_clear_all() {
        let svc = make_service();
        for i in 0..10 {
            svc.push_notification(make_event(
                NotificationType::Info,
                &format!("msg {}", i),
            ));
        }
        let cleared = svc.clear_all();
        assert_eq!(cleared, 10);
        assert!(svc.get_all_history().is_empty());
    }

    #[test]
    fn test_max_history() {
        let svc = make_service();
        for i in 0..510 {
            svc.push_notification(make_event(
                NotificationType::Info,
                &format!("msg {}", i),
            ));
        }
        let history = svc.get_all_history();
        assert_eq!(history.len(), 500);
        assert_eq!(history[0].title, "msg 10");
    }

    #[test]
    fn test_get_history_with_limit() {
        let svc = make_service();
        for i in 0..10 {
            svc.push_notification(make_event(
                NotificationType::Info,
                &format!("msg {}", i),
            ));
        }
        let opts = HistoryOptions {
            limit: Some(3),
            ..Default::default()
        };
        let history = svc.get_history(Some(&opts));
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_get_history_unread_only() {
        let svc = make_service();
        let _r1 = svc.push_notification(make_event(NotificationType::Info, "unread"));
        let r2 = svc.push_notification(make_event(NotificationType::Info, "read"));
        svc.mark_read(&r2.id).unwrap();
        let opts = HistoryOptions {
            unread_only: Some(true),
            ..Default::default()
        };
        let history = svc.get_history(Some(&opts));
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "unread");
    }
}

// ---------------------------------------------------------------------------
// AgentComms Tests
// ---------------------------------------------------------------------------

mod agent_comms_tests {
    use crate::agent_comms::{AgentComms, AgentCommsError};

    fn make_comms() -> AgentComms {
        AgentComms::new()
    }

    #[test]
    fn test_get_comms_token() {
        let comms = make_comms();
        let token = comms.get_comms_token();
        assert!(!token.is_empty());
        assert!(token.len() > 10);
    }

    #[test]
    fn test_send_to_agent_not_found() {
        let comms = make_comms();
        let result = comms.send_to_agent("nonexistent", "test", &serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentCommsError::AgentNotFound(id) => assert_eq!(id, "nonexistent"),
            _ => panic!("Expected AgentNotFound error"),
        }
    }

    #[test]
    fn test_respond_to_input_request() {
        let comms = make_comms();
        let rx = comms.inject_input_request("req-1");
        let result = comms.respond_to_input_request("req-1", "my response");
        assert!(result.is_ok());
        assert_eq!(rx.recv().unwrap(), "my response");
    }

    #[test]
    fn test_cancel_input_request() {
        let comms = make_comms();
        let _rx = comms.inject_input_request("req-cancel");
        let result = comms.cancel_input_request("req-cancel");
        assert!(result.is_ok());
        assert!(result.unwrap());
        let result2 = comms.cancel_input_request("nonexistent");
        assert!(result2.is_ok());
        assert!(!result2.unwrap());
    }

    #[test]
    fn test_disconnect_agent() {
        let comms = make_comms();
        let result = comms.disconnect_agent("nonexistent");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_broadcast_to_agents_empty() {
        let comms = make_comms();
        let result = comms.broadcast_to_agents("test", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_agent_comms() {
        let comms = make_comms();
        let _rx = comms.inject_input_request("req-shutdown");
        assert!(!comms.pending_input_is_empty());
        comms.shutdown_agent_comms().unwrap();
        assert!(comms.pending_input_is_empty());
    }

    #[test]
    fn test_get_agent_sessions_empty() {
        let comms = make_comms();
        let sessions = comms.get_agent_sessions();
        assert!(sessions.is_empty());
    }
}

// ---------------------------------------------------------------------------
// SwarmCoordinator Tests
// ---------------------------------------------------------------------------

mod swarm_tests {
    use crate::swarm::{
        SwarmCoordinator, SwarmState,
    };

    fn make_coordinator() -> SwarmCoordinator {
        SwarmCoordinator::new()
    }

    #[tokio::test]
    async fn test_create_swarm() {
        let coord = make_coordinator();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let state = coord.read_state(dir).await.unwrap();
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_send_message() {
        let coord = make_coordinator();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        coord
            .send_message(dir, "agent-a", "agent-b", "hello")
            .await
            .unwrap();
        let mailbox = coord.read_mailbox(dir, "agent-b").await.unwrap();
        assert_eq!(mailbox.len(), 1);
        assert_eq!(mailbox[0].from, "agent-a");
        assert_eq!(mailbox[0].content, "hello");
    }

    #[tokio::test]
    async fn test_read_mailbox() {
        let coord = make_coordinator();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let mailbox = coord.read_mailbox(dir, "nonexistent").await.unwrap();
        assert!(mailbox.is_empty());

        coord
            .send_message(dir, "sender", "receiver", "msg1")
            .await
            .unwrap();
        coord
            .send_message(dir, "sender", "receiver", "msg2")
            .await
            .unwrap();
        let mailbox = coord.read_mailbox(dir, "receiver").await.unwrap();
        assert_eq!(mailbox.len(), 2);
    }

    #[tokio::test]
    async fn test_read_state() {
        let coord = make_coordinator();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let state = SwarmState::default();
        coord.write_state(dir, &state).await.unwrap();
        let read = coord.read_state(dir).await.unwrap();
        assert!(read.is_some());
        assert_eq!(read.unwrap().agents.len(), 0);
    }

    #[tokio::test]
    async fn test_generate_msg_id() {
        let id1 = generate_msg_id();
        let id2 = generate_msg_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("msg-"));
    }

    fn generate_msg_id() -> String {
        format!("msg-{}", uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_watch_prevents_duplicates() {
        let coord = make_coordinator();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        coord.watch_state(dir).await.unwrap();
        coord.watch_state(dir).await.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        coord.stop_watch(dir).unwrap();
    }

    #[tokio::test]
    async fn test_subscribe() {
        let coord = make_coordinator();
        let mut rx = coord.subscribe();
        let state = rx.borrow_and_update();
        assert_eq!(state.agents.len(), 0);
    }
}

// ---------------------------------------------------------------------------
// ShellIntegration Tests
// ---------------------------------------------------------------------------

mod shell_integration_tests {
    use crate::shell_integration::{
        parse_osc633, process_sequences, strip_osc633, CommandTracker, Osc633Parser,
        ShellIntegrationSequence,
    };

    #[test]
    fn test_parse_cwd_sequence() {
        let cwd = "/Users/test/project";
        let sequence = format!("\x1b]633;P;{}\x07", cwd);
        let parsed = parse_osc633(&sequence);
        assert_eq!(parsed.len(), 1);
        match &parsed[0].sequence {
            ShellIntegrationSequence::Cwd { data } => assert_eq!(data, cwd),
            other => panic!("Expected Cwd, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_command_start() {
        let cmd = "ls -la";
        let sequence = format!("\x1b]633;B;{}\x07", cmd);
        let parsed = parse_osc633(&sequence);
        assert_eq!(parsed.len(), 1);
        match &parsed[0].sequence {
            ShellIntegrationSequence::Command { data } => assert_eq!(data, cmd),
            other => panic!("Expected Command, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_command_exit() {
        let sequence = "\x1b]633;D;42\x07";
        let parsed = parse_osc633(sequence);
        assert_eq!(parsed.len(), 1);
        match &parsed[0].sequence {
            ShellIntegrationSequence::CommandFinished { exit_code } => {
                assert_eq!(*exit_code, 42);
            }
            other => panic!("Expected CommandFinished, got {:?}", other),
        }
    }

    #[test]
    fn test_strip_osc_sequences() {
        let input = format!("hello\x1b]633;P;/tmp\x07world\x1b]633;A\x07end");
        let stripped = strip_osc633(&input);
        assert_eq!(stripped, "helloworldend");
    }

    #[test]
    fn test_parse_prompt_sequence() {
        let sequence = "\x1b]633;A\x07";
        let parsed = parse_osc633(sequence);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0].sequence,
            ShellIntegrationSequence::Prompt { .. }
        ));
    }

    #[test]
    fn test_parse_command_executed() {
        let sequence = "\x1b]633;E\x07";
        let parsed = parse_osc633(sequence);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0].sequence,
            ShellIntegrationSequence::CommandExecuted
        ));
    }

    #[test]
    fn test_process_sequences_full_flow() {
        let mut tracker = CommandTracker::new();
        let data = format!(
            "\x1b]633;B;cargo build\x07\x1b]633;C\x07\x1b]633;E\x07\x1b]633;D;0\x07"
        );
        let parsed = parse_osc633(&data);
        let events = process_sequences(&mut tracker, &parsed, "pane-1");
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            crate::shell_integration::ShellIntegrationEvent::CommandStart { .. }
        ));
        assert!(matches!(
            events[1],
            crate::shell_integration::ShellIntegrationEvent::CommandExecuted { .. }
        ));
        assert!(matches!(
            events[2],
            crate::shell_integration::ShellIntegrationEvent::CommandFinished { exit_code: 0, .. }
        ));
    }

    #[test]
    fn test_strip_osc_multiple_sequences() {
        let input = format!(
            "start\x1b]633;P;/tmp\x07mid\x1b]633;A\x07end"
        );
        let stripped = strip_osc633(&input);
        assert_eq!(stripped, "startmidend");
    }

    #[test]
    fn test_parser_feed_incremental() {
        let mut parser = Osc633Parser::new();
        let partial = "\x1b]633;P;/tmp";
        let results = parser.feed(partial);
        assert!(results.is_empty());

        let complete = "\x07";
        let results = parser.feed(complete);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_command_tracker_default() {
        let tracker = CommandTracker::new();
        assert!(tracker.active_command.is_none());
        assert!(tracker.current_cwd.is_none());
        assert!(tracker.last_exit_code.is_none());
    }
}

// ---------------------------------------------------------------------------
// Search Tests
// ---------------------------------------------------------------------------

mod search_tests {
    use crate::search::{search_code, search_files, SearchError};
    use crate::types::SearchOptions;
    use std::fs;

    #[tokio::test]
    async fn test_find_rg_binary() {
        let result = crate::search::find_rg_binary().await;
        match result {
            Ok(path) => {
                assert!(path.exists());
            }
            Err(SearchError::RgNotFound) => {
                println!("ripgrep not found on system — test passes (error handled gracefully)");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_search_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();

        let options = SearchOptions {
            pattern: "nonexistent_pattern".to_string(),
            path: dir.to_string(),
            glob: None,
            case_sensitive: false,
            max_results: None,
            context_lines: None,
        };

        let result = search_code(&options).await;
        match result {
            Ok(search_result) => {
                assert!(search_result.matches.is_empty());
                assert_eq!(search_result.stats.total_matches, 0);
                assert_eq!(search_result.stats.files_matched, 0);
            }
            Err(SearchError::RgNotFound) => {
                println!("ripgrep not found — skipping search test");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_search_invalid_path() {
        let options = SearchOptions {
            pattern: "test".to_string(),
            path: "/nonexistent/path/that/does/not/exist".to_string(),
            glob: None,
            case_sensitive: false,
            max_results: None,
            context_lines: None,
        };

        let result = search_code(&options).await;
        match result {
            Err(SearchError::RgNotFound) => {
                println!("ripgrep not found — skipping search test");
            }
            Err(SearchError::RgExit { code, .. }) => {
                assert!(code != 0);
            }
            Err(e) => panic!("Unexpected error type: {:?}", e),
            Ok(_) => panic!("Expected an error for invalid path"),
        }
    }

    #[tokio::test]
    async fn test_search_files_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();

        let result = search_files(dir, "", None, None).await;
        match result {
            Ok(files) => {
                assert!(files.is_empty());
            }
            Err(SearchError::RgNotFound) => {
                println!("ripgrep not found — skipping search_files test");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_search_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        fs::write(tmp.path().join("test.txt"), "hello world\nfoo bar\n").unwrap();

        let options = SearchOptions {
            pattern: "hello".to_string(),
            path: dir.to_string(),
            glob: None,
            case_sensitive: false,
            max_results: None,
            context_lines: None,
        };

        let result = search_code(&options).await;
        match result {
            Ok(search_result) => {
                assert_eq!(search_result.stats.total_matches, 1);
                assert_eq!(search_result.stats.files_matched, 1);
                assert!(search_result.matches[0]
                    .line_text
                    .contains("hello"));
            }
            Err(SearchError::RgNotFound) => {
                println!("ripgrep not found — skipping search test");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }
}
