use uuid::Uuid;

use crate::persistence::Persistence;
use crate::types::{InstancePhase, ServerInstanceConfig};

// ─── InstanceRecord ──────────────────────────────────────────────────────────

pub struct InstanceRecord {
    pub config: ServerInstanceConfig,
    pub phase: InstancePhase,
}

// ─── InstanceRegistry ────────────────────────────────────────────────────────

pub struct InstanceRegistry {
    instances: Vec<InstanceRecord>,
    persistence: Box<dyn Persistence>,
}

impl InstanceRegistry {
    pub fn new(persistence: Box<dyn Persistence>) -> Self {
        Self { instances: Vec::new(), persistence }
    }

    pub fn add_instance(&mut self, config: ServerInstanceConfig) -> Uuid {
        let id = config.id;
        self.instances.push(InstanceRecord { config, phase: InstancePhase::Stopped });
        id
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

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }
}
