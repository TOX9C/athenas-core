pub mod ansi;
pub mod grid;
pub mod input;
pub mod session;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionConfig {
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

pub type GridRef = Arc<Mutex<grid::Grid>>;
