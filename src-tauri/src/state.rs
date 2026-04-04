use std::collections::HashMap;
use tauri_plugin_shell::process::CommandChild;

pub struct AppState {
    pub active_jobs: HashMap<String, (CommandChild, String)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_jobs: HashMap::new(),
        }
    }
}
