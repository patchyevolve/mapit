use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use mapit_core::graph::store::GraphStore;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<GraphStore>>,
    pub ws_tx: broadcast::Sender<String>,
    pub progress: Arc<Mutex<ProgressState>>,
    pub project_root: PathBuf,
    pub mapit_dir: PathBuf,
    pub cancel_flag: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct ProgressState {
    pub phase: String,
    pub current: u64,
    pub total: u64,
    pub current_file: Option<String>,
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            current: 0,
            total: 0,
            current_file: None,
        }
    }
}
