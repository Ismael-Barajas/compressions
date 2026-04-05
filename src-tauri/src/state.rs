use std::collections::{HashMap, HashSet};
use tauri_plugin_shell::process::CommandChild;

#[derive(Default)]
pub struct AppState {
    pub active_jobs: HashMap<String, (CommandChild, String)>,
    pub hw_encoders: HashSet<String>,
}
