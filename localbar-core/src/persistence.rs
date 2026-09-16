use std::collections::HashMap;
use std::path::PathBuf;

use crate::types::{ModelMemory, ModelMemoryKey, NamedProfile, ServerInstanceConfig};

// ─── Persistence trait ───────────────────────────────────────────────────────

pub trait Persistence: Send + Sync {
    fn save_instances(&mut self, configs: &[ServerInstanceConfig]) -> Result<(), String>;
    fn load_instances(&self) -> Result<Vec<ServerInstanceConfig>, String>;
    fn save_profiles(&mut self, profiles: &[NamedProfile]) -> Result<(), String>;
    fn load_profiles(&self) -> Result<Vec<NamedProfile>, String>;
    fn load_model_memory(&self) -> Result<HashMap<ModelMemoryKey, ModelMemory>, String>;
    fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String>;
}

// ─── InMemoryPersistence ─────────────────────────────────────────────────────

/// In-memory persistence adapter for tests. No filesystem access.
#[derive(Debug, Default)]
pub struct InMemoryPersistence {
    instances: Vec<ServerInstanceConfig>,
    profiles: Vec<NamedProfile>,
    model_memory: HashMap<ModelMemoryKey, ModelMemory>,
}

impl Persistence for InMemoryPersistence {
    fn save_instances(&mut self, configs: &[ServerInstanceConfig]) -> Result<(), String> {
        self.instances = configs.to_vec();
        Ok(())
    }

    fn load_instances(&self) -> Result<Vec<ServerInstanceConfig>, String> {
        Ok(self.instances.clone())
    }

    fn save_profiles(&mut self, profiles: &[NamedProfile]) -> Result<(), String> {
        self.profiles = profiles.to_vec();
        Ok(())
    }

    fn load_profiles(&self) -> Result<Vec<NamedProfile>, String> {
        Ok(self.profiles.clone())
    }

    fn load_model_memory(&self) -> Result<HashMap<ModelMemoryKey, ModelMemory>, String> {
        Ok(self.model_memory.clone())
    }

    fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String> {
        self.model_memory.insert(entry.key(), entry);
        Ok(())
    }
}

// ─── FilePersistence ─────────────────────────────────────────────────────────

/// JSON-file persistence. Reads the whole file on load; writes the whole file on save.
/// All fields share one file — each save operation preserves the other fields.
pub struct FilePersistence {
    path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct StorageFile {
    #[serde(default)]
    instances: Vec<ServerInstanceConfig>,
    #[serde(default)]
    profiles: Vec<NamedProfile>,
    #[serde(default)]
    model_memory: HashMap<ModelMemoryKey, ModelMemory>,
}

impl FilePersistence {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<StorageFile, String> {
        match std::fs::read_to_string(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StorageFile::default()),
            Err(e) => Err(format!("state.json read error: {e}")),
            Ok(s) => serde_json::from_str(&s).map_err(|e| format!("state.json corrupt: {e}")),
        }
    }

    fn write(&mut self, file: &StorageFile) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        serde_json::to_string_pretty(file)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(&self.path, json).map_err(|e| e.to_string()))
    }
}

impl Persistence for FilePersistence {
    fn save_instances(&mut self, configs: &[ServerInstanceConfig]) -> Result<(), String> {
        let mut file = self.read()?;
        file.instances = configs.to_vec();
        self.write(&file)
    }

    fn load_instances(&self) -> Result<Vec<ServerInstanceConfig>, String> {
        Ok(self.read()?.instances)
    }

    fn save_profiles(&mut self, profiles: &[NamedProfile]) -> Result<(), String> {
        let mut file = self.read()?;
        file.profiles = profiles.to_vec();
        self.write(&file)
    }

    fn load_profiles(&self) -> Result<Vec<NamedProfile>, String> {
        Ok(self.read()?.profiles)
    }

    fn load_model_memory(&self) -> Result<HashMap<ModelMemoryKey, ModelMemory>, String> {
        Ok(self.read()?.model_memory)
    }

    fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String> {
        let mut file = self.read()?;
        file.model_memory.insert(entry.key(), entry);
        self.write(&file)
    }
}

// ─── FilePersistence tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{NamedProfile, ParamValues, ServerInstanceConfig, ServerType};
    use uuid::Uuid;

    fn ollama_config() -> ServerInstanceConfig {
        ServerInstanceConfig::new("ollama", ServerType::Ollama, 11434, "/usr/bin/ollama")
    }

    fn named_profile() -> NamedProfile {
        NamedProfile { id: Uuid::new_v4(), name: "p".into(), params: ParamValues::default(), server_type: None }
    }

    #[test]
    fn file_persistence_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = FilePersistence::new(dir.path().join("state.json"));
        let config = ollama_config();
        p.save_instances(std::slice::from_ref(&config)).unwrap();
        let loaded = p.load_instances().unwrap();
        assert_eq!(loaded, vec![config]);
    }

    #[test]
    fn file_persistence_empty_on_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = FilePersistence::new(dir.path().join("nonexistent.json"));
        assert!(p.load_instances().unwrap().is_empty());
    }

    #[test]
    fn file_persistence_error_on_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, b"not valid json {{{{").unwrap();
        let p = FilePersistence::new(path);
        assert!(p.load_instances().is_err());
    }

    #[test]
    fn file_persistence_save_instances_preserves_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = FilePersistence::new(dir.path().join("state.json"));
        p.save_profiles(&[named_profile()]).unwrap();
        p.save_instances(&[ollama_config()]).unwrap();
        assert_eq!(p.load_profiles().unwrap().len(), 1);
    }

    #[test]
    fn file_persistence_creates_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = FilePersistence::new(dir.path().join("sub/dir/state.json"));
        p.save_instances(&[ollama_config()]).unwrap();
        assert_eq!(p.load_instances().unwrap().len(), 1);
    }
}
