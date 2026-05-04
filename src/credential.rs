use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialStore {
    pub entries: HashMap<String, String>,
}

impl CredentialStore {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get_credential(&self, provider_id: &str) -> Option<&str> {
        self.entries.get(provider_id).map(|s| s.as_str())
    }

    pub fn set_credential(&mut self, provider_id: String, credential: String) {
        self.entries.insert(provider_id, credential);
    }

    pub fn remove_credential(&mut self, provider_id: &str) {
        self.entries.remove(provider_id);
    }

    pub fn has_credential(&self, provider_id: &str) -> bool {
        self.entries.contains_key(provider_id)
    }
}
