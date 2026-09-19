use std::collections::HashMap;

use uuid::Uuid;

use crate::persistence::Persistence;
use crate::types::{InstancePhase, ModelMemory, ModelMemoryKey, NamedProfile, ParamValues, ServerInstanceConfig};

// ─── InstanceRecord ──────────────────────────────────────────────────────────

pub struct InstanceRecord {
    pub config: ServerInstanceConfig,
    pub phase: InstancePhase,
}

// ─── InstanceRegistry ────────────────────────────────────────────────────────

pub struct InstanceRegistry {
    instances: Vec<InstanceRecord>,
    profiles: Vec<NamedProfile>,
    model_memory: HashMap<ModelMemoryKey, ModelMemory>,
    persistence: Box<dyn Persistence>,
}

impl InstanceRegistry {
    pub fn new(persistence: Box<dyn Persistence>) -> Self {
        Self {
            instances: Vec::new(),
            profiles: Vec::new(),
            model_memory: HashMap::new(),
            persistence,
        }
    }

    pub fn add_instance(&mut self, config: ServerInstanceConfig) -> Uuid {
        let id = config.id;
        self.instances.push(InstanceRecord { config, phase: InstancePhase::Stopped });
        id
    }

    pub fn remove_instance(&mut self, id: Uuid) -> Result<(), String> {
        let pos = self.instances.iter().position(|r| r.config.id == id)
            .ok_or_else(|| format!("remove_instance: no instance with id {id}"))?;
        self.instances.remove(pos);
        Ok(())
    }

    pub fn set_phase(&mut self, id: Uuid, phase: InstancePhase) -> Result<(), String> {
        match self.instances.iter_mut().find(|r| r.config.id == id) {
            Some(r) => { r.phase = phase; Ok(()) }
            None => Err(format!("set_phase: no instance with id {id}")),
        }
    }

    pub fn get_phase(&self, id: Uuid) -> Option<&InstancePhase> {
        self.instances.iter().find(|r| r.config.id == id).map(|r| &r.phase)
    }

    pub fn get_config(&self, id: Uuid) -> Option<&ServerInstanceConfig> {
        self.instances.iter().find(|r| r.config.id == id).map(|r| &r.config)
    }

    pub fn update_config<F: FnOnce(&mut ServerInstanceConfig)>(
        &mut self,
        id: Uuid,
        f: F,
    ) -> Result<(), String> {
        match self.instances.iter_mut().find(|r| r.config.id == id) {
            Some(r) => { f(&mut r.config); Ok(()) }
            None => Err(format!("update_config: no instance with id {id}")),
        }
    }

    pub fn all_configs(&self) -> impl Iterator<Item = &ServerInstanceConfig> {
        self.instances.iter().map(|r| &r.config)
    }

    pub fn any_running(&self) -> bool {
        self.instances.iter().any(|r| r.phase.is_active())
    }

    /// Returns a warning string when starting this instance would conflict with
    /// another active instance, or None when it is safe to start (C3).
    /// Callers receive the message or None — no policy logic outside this method.
    pub fn start_warning(&self, instance_id: Uuid) -> Option<String> {
        self.instances
            .iter()
            .find(|r| r.config.id != instance_id && r.phase.is_active())
            .map(|r| {
                format!(
                    "\"{}\" is already running. Starting another instance at the same time may cause conflicts.",
                    r.config.name
                )
            })
    }

    /// Persist current instance configs to the backing store.
    pub fn save(&mut self) -> Result<(), String> {
        let configs: Vec<_> = self.instances.iter().map(|r| r.config.clone()).collect();
        self.persistence.save_instances(&configs)
    }

    /// Load instance configs from the backing store. All phases start at Stopped.
    pub fn load(&mut self) -> Result<(), String> {
        let configs = self.persistence.load_instances()?;
        self.instances = configs
            .into_iter()
            .map(|config| InstanceRecord { phase: InstancePhase::Stopped, config })
            .collect();
        Ok(())
    }

    /// Load named profiles from the backing store into the in-memory cache.
    pub fn load_profiles(&mut self) -> Result<(), String> {
        self.profiles = self.persistence.load_profiles()?;
        Ok(())
    }

    /// Load model memory from the backing store into the in-memory cache.
    pub fn load_model_memory(&mut self) -> Result<(), String> {
        self.model_memory = self.persistence.load_model_memory()?;
        Ok(())
    }

    /// Return the resolved-params layer for the active profile of the given instance, if any.
    pub fn get_active_profile_params(&self, instance_id: Uuid) -> Option<&ParamValues> {
        let record = self.instances.iter().find(|r| r.config.id == instance_id)?;
        let profile_id = record.config.active_profile_id?;
        self.profiles.iter().find(|p| p.id == profile_id).map(|p| &p.params)
    }

    /// Return the cached `ModelMemory` for the given key, if any.
    pub fn get_model_memory(&self, key: &ModelMemoryKey) -> Option<&ModelMemory> {
        self.model_memory.get(key)
    }

    /// Persist and cache a model-memory entry.
    pub fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String> {
        self.persistence.upsert_model_memory(entry.clone())?;
        self.model_memory.insert(entry.key(), entry);
        Ok(())
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }
}

// ─── Registry tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::InMemoryPersistence;
    use crate::types::{InstanceError, InstanceErrorKind, InstancePhase, ServerType};

    fn make_registry() -> InstanceRegistry {
        InstanceRegistry::new(Box::new(InMemoryPersistence::default()))
    }

    fn ollama_config(name: &str) -> ServerInstanceConfig {
        ServerInstanceConfig::new(name, ServerType::Ollama, 11434, "/usr/bin/ollama")
    }

    #[test]
    fn remove_instance_decrements_count() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.add_instance(ollama_config("b"));
        assert_eq!(reg.instance_count(), 2);
        reg.remove_instance(id).unwrap();
        assert_eq!(reg.instance_count(), 1);
    }

    #[test]
    fn remove_instance_unknown_id_returns_err() {
        let mut reg = make_registry();
        assert!(reg.remove_instance(uuid::Uuid::new_v4()).is_err());
    }

    #[test]
    fn get_config_missing_after_remove() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.remove_instance(id).unwrap();
        assert!(reg.get_config(id).is_none());
    }

    #[test]
    fn all_configs_returns_all() {
        let mut reg = make_registry();
        reg.add_instance(ollama_config("a"));
        reg.add_instance(ollama_config("b"));
        let names: Vec<_> = reg.all_configs().map(|c| c.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn any_running_false_when_all_stopped() {
        let mut reg = make_registry();
        reg.add_instance(ollama_config("a"));
        assert!(!reg.any_running());
    }

    #[test]
    fn any_running_true_when_one_active() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        assert!(reg.any_running());
    }

    #[test]
    fn update_config_mutates_field() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("original"));
        reg.update_config(id, |c| c.name = "updated".into()).unwrap();
        assert_eq!(reg.get_config(id).unwrap().name, "updated");
    }

    #[test]
    fn update_config_unknown_id_returns_err() {
        let mut reg = make_registry();
        assert!(reg.update_config(uuid::Uuid::new_v4(), |c| c.name = "x".into()).is_err());
    }

    #[test]
    fn remove_clears_phase_from_registry() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        reg.remove_instance(id).unwrap();
        assert!(reg.get_phase(id).is_none());
    }

    #[test]
    fn any_running_false_after_error_phase() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Error(InstanceError {
            kind: InstanceErrorKind::LaunchFailed,
            message: "test".into(),
        })).unwrap();
        assert!(!reg.any_running());
    }
}
