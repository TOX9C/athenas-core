use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("test-pty-{}", n)
}

fn test_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn default_shell_path() -> String {
    #[cfg(target_os = "windows")]
    {
        "powershell.exe".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
    }
}

#[test]
fn test_new_manager() {
    let manager = SessionManager::new();
    assert!(!manager.has_session("any"));
    assert_eq!(manager.get_history("any"), "");
}

#[test]
fn test_spawn_session() {
    let manager = SessionManager::new();
    let id = unique_id();
    let result = manager.spawn(id.clone(), test_cwd(), default_shell_path(), None);
    assert!(result.is_ok());
    assert!(manager.has_session(&id));
    manager.kill(&id);
}

#[test]
fn test_has_session() {
    let manager = SessionManager::new();
    let id = unique_id();
    manager
        .spawn(id.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    assert!(manager.has_session(&id));
    manager.kill(&id);
}

#[test]
fn test_has_session_false() {
    let manager = SessionManager::new();
    assert!(!manager.has_session("nonexistent-session"));
}

#[test]
fn test_write_to_session() {
    let manager = SessionManager::new();
    let id = unique_id();
    manager
        .spawn(id.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    // Give the shell a moment to start
    std::thread::sleep(std::time::Duration::from_millis(200));
    let result = manager.write(&id, "echo hello\r".to_string());
    assert!(result.is_ok());
    manager.kill(&id);
}

#[test]
fn test_write_nonexistent() {
    let manager = SessionManager::new();
    let result = manager.write("nonexistent", "data".to_string());
    assert!(result.is_err());
}

#[test]
fn test_resize_session() {
    let manager = SessionManager::new();
    let id = unique_id();
    manager
        .spawn(id.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    let result = manager.resize(&id, 120, 40);
    assert!(result.is_ok());
    manager.kill(&id);
}

#[test]
fn test_resize_nonexistent() {
    let manager = SessionManager::new();
    let result = manager.resize("nonexistent", 80, 24);
    assert!(result.is_err());
}

#[test]
fn test_kill_session() {
    let manager = SessionManager::new();
    let id = unique_id();
    manager
        .spawn(id.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    assert!(manager.has_session(&id));
    manager.kill(&id);
    assert!(!manager.has_session(&id));
}

#[test]
fn test_kill_nonexistent() {
    let manager = SessionManager::new();
    // Should not panic
    manager.kill("nonexistent");
}

#[test]
fn test_multiple_sessions() {
    let manager = SessionManager::new();
    let id1 = unique_id();
    let id2 = unique_id();
    let id3 = unique_id();
    manager
        .spawn(id1.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    manager
        .spawn(id2.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    manager
        .spawn(id3.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    assert!(manager.has_session(&id1));
    assert!(manager.has_session(&id2));
    assert!(manager.has_session(&id3));
    manager.kill(&id1);
    manager.kill(&id2);
    manager.kill(&id3);
}

#[test]
fn test_graceful_shutdown() {
    let manager = SessionManager::new();
    let id1 = unique_id();
    let id2 = unique_id();
    manager
        .spawn(id1.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    manager
        .spawn(id2.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    assert!(manager.has_session(&id1));
    assert!(manager.has_session(&id2));
    manager.graceful_shutdown();
    assert!(!manager.has_session(&id1));
    assert!(!manager.has_session(&id2));
}

#[test]
fn test_data_callback() {
    let received: Arc<StdMutex<Vec<(String, Vec<u8>)>>> = Arc::new(StdMutex::new(Vec::new()));
    let manager = SessionManager::new_with_data_callback({
        let received = Arc::clone(&received);
        move |_id: &str, data: &[u8]| {
            if !data.is_empty() {
                received.lock().unwrap().push((_id.to_string(), data.to_vec()));
            }
        }
    });
    let id = unique_id();
    manager
        .spawn(id.clone(), test_cwd(), default_shell_path(), None)
        .unwrap();
    // Wait for shell to produce output (prompt, etc.)
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let data = received.lock().unwrap();
    // Shell should have produced some output (at least a prompt)
    assert!(!data.is_empty(), "Expected shell to produce output");
    manager.kill(&id);
}

#[test]
fn test_default_shell() {
    let shell = default_shell();
    // On macOS/Linux, should be a valid shell path
    #[cfg(not(target_os = "windows"))]
    {
        assert!(
            shell.contains("zsh") || shell.contains("bash") || shell.contains("sh"),
            "Expected shell path, got: {}",
            shell
        );
    }
    #[cfg(target_os = "windows")]
    {
        assert_eq!(shell, "powershell.exe");
    }
}
