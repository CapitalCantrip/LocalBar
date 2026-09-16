use std::collections::HashMap;

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
