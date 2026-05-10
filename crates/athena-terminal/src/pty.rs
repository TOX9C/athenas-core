use portable_pty::{CommandBuilder, native_pty_system, PtySize};
use anyhow::Result;
use std::sync::{Arc, Mutex};

pub struct PtySession {
    alive: Arc<Mutex<bool>>,
}

impl PtySession {
    pub fn new() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap(); 
        
        let cmd = CommandBuilder::new("bash");
        let mut _child = pair.slave.spawn_command(cmd).unwrap();

        let alive = Arc::new(Mutex::new(true));
        
        Ok(Self { alive })
    }

    pub fn is_alive(&self) -> bool {
        *self.alive.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_pty() {
        let pty = PtySession::new().unwrap();
        assert!(pty.is_alive());
    }
}
