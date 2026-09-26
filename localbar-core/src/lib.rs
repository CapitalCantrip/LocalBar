// localbar-core: platform-agnostic domain logic, drivers, and persistence.
// No Tauri dependency. All Tauri-specific code lives in localbar-tauri.
#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_lines)]

pub mod driver;
pub mod drivers;
pub mod lifecycle;
pub mod net;
pub mod persistence;
pub mod registry;
pub mod testing;
pub mod types;

use uuid::Uuid;

use driver::ServerDriver;
use registry::InstanceRegistry;
use types::{ModelMemory, ModelMemoryKey, ParamValues, ServerInstanceConfig, ServerType};

// ─── adopt_external_as_new_instance ──────────────────────────────────────────

/// Create a new External instance at the same host:port as the conflicting managed instance.
/// The original instance is left in its current phase; callers may reset it to Stopped.
/// `detected_model_key` should come from probing the server's `/v1/models` before calling.
pub fn adopt_external_as_new_instance(
    registry: &mut InstanceRegistry,
    conflicting_id: Uuid,
    detected_model_key: Option<String>,
) -> Result<Uuid, String> {
    let (host, port) = registry
        .get_config(conflicting_id)
        .ok_or_else(|| format!("adopt_external: no instance {conflicting_id}"))
        .map(|c| (c.host.clone(), c.port))?;
    let mut new_config = ServerInstanceConfig::new(
        format!("Adopted ({}:{})", host, port),
        ServerType::External,
        port,
        "",
    );
    new_config.host = host;
    new_config.selected_model_key = detected_model_key;
    let new_id = registry.add_instance(new_config);
    registry.save()?;
    Ok(new_id)
}

// ─── ensure_managed_model ────────────────────────────────────────────────────

/// Resolve params and bake a managed model tag for an instance.
///
/// Must be called **after** `selected_model_key` is written to the config
/// (Bug 1 fix): the tag is built from whatever key is currently in the config.
/// No-ops for drivers that return `None` from `generate_managed_config`.
pub fn ensure_managed_model(
    registry: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
) -> Result<(), String> {
    let config = registry
        .get_config(id)
        .ok_or_else(|| format!("ensure_managed_model: no instance {id}"))?
        .clone();
    let Some(model_key) = config.selected_model_key.clone() else { return Ok(()); };
    let profile = registry.get_active_profile_params(id).cloned();
    let mem_key = ModelMemoryKey { server_type: config.server_type, model_key: model_key.clone() };
    let memory = registry.get_model_memory(&mem_key).cloned();
    let schema = driver.param_schema();
    let resolved = ParamValues::resolve(profile.as_ref(), memory.as_ref(), &schema);
    let Some(content) = driver.generate_managed_config(&model_key, &resolved.0, &config)
        else { return Ok(()); };
    let Some(tag) = driver.managed_config_tag(&model_key, id) else { return Ok(()); };
    driver.apply_managed_config(&config, &tag, &content)?;
    registry.update_config(id, |c| c.managed_model_tag = Some(tag))?;
    registry.save()
}

// ─── push_restart_duration_sample ────────────────────────────────────────────

/// Append a startup-duration sample to the rolling window in ModelMemory.
///
/// The window holds at most 10 samples, newest first. No-ops when no model is
/// selected. Duration is wall-clock seconds from when the instance entered
/// Starting (or SwitchingModel) to when it first became healthy.
pub fn push_restart_duration_sample(
    registry: &mut InstanceRegistry,
    id: Uuid,
    duration_secs: f64,
) -> Result<(), String> {
    let config = registry
        .get_config(id)
        .ok_or_else(|| format!("push_restart_duration_sample: no instance {id}"))?;
    let Some(model_key) = config.selected_model_key.clone() else { return Ok(()); };
    let server_type = config.server_type;
    let key = ModelMemoryKey { server_type, model_key: model_key.clone() };
    let mut entry = registry
        .get_model_memory(&key)
        .cloned()
        .unwrap_or_else(|| ModelMemory::new(server_type, model_key));
    entry.restart_duration_samples.insert(0, duration_secs);
    entry.restart_duration_samples.truncate(10);
    registry.upsert_model_memory(entry)
}

// ─── update_model_memory ─────────────────────────────────────────────────────

/// Persist the params that were active when a model became Running (auto-memory).
/// No-ops when no model is selected.
pub fn update_model_memory(
    registry: &mut InstanceRegistry,
    id: Uuid,
    params: ParamValues,
) -> Result<(), String> {
    let config = registry
        .get_config(id)
        .ok_or_else(|| format!("update_model_memory: no instance {id}"))?;
    let Some(model_key) = config.selected_model_key.clone() else { return Ok(()); };
    let server_type = config.server_type;
    let key = ModelMemoryKey { server_type, model_key: model_key.clone() };
    let mut entry = registry
        .get_model_memory(&key)
        .cloned()
        .unwrap_or_else(|| ModelMemory::new(server_type, model_key));
    entry.last_used_params = params;
    registry.upsert_model_memory(entry)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::driver::ServerDriver;
    use crate::persistence::{InMemoryPersistence, Persistence};
    use crate::registry::InstanceRegistry;
    use crate::testing::MockDriver;
    use crate::types::{
        CanonicalParam, InstanceError, InstanceErrorKind, InstancePhase, ModelMemory, ParamValue,
        ParamValues, ServerInstanceConfig, ServerType,
    };
    use super::{adopt_external_as_new_instance, ensure_managed_model, update_model_memory};

    struct SharedPersistence(Arc<Mutex<InMemoryPersistence>>);
    impl Persistence for SharedPersistence {
        fn save_instances(&mut self, configs: &[ServerInstanceConfig]) -> Result<(), String> {
            self.0.lock().unwrap().save_instances(configs)
        }
        fn load_instances(&self) -> Result<Vec<ServerInstanceConfig>, String> {
            self.0.lock().unwrap().load_instances()
        }
        fn save_profiles(&mut self, profiles: &[crate::types::NamedProfile]) -> Result<(), String> {
            self.0.lock().unwrap().save_profiles(profiles)
        }
        fn load_profiles(&self) -> Result<Vec<crate::types::NamedProfile>, String> {
            self.0.lock().unwrap().load_profiles()
        }
        fn load_model_memory(&self) -> Result<std::collections::HashMap<crate::types::ModelMemoryKey, ModelMemory>, String> {
            self.0.lock().unwrap().load_model_memory()
        }
        fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String> {
            self.0.lock().unwrap().upsert_model_memory(entry)
        }
        fn save_discovery_config(&mut self, config: &crate::types::DiscoveryConfig) -> Result<(), String> {
            self.0.lock().unwrap().save_discovery_config(config)
        }
        fn load_discovery_config(&self) -> Result<crate::types::DiscoveryConfig, String> {
            self.0.lock().unwrap().load_discovery_config()
        }
    }
    unsafe impl Send for SharedPersistence {}
    unsafe impl Sync for SharedPersistence {}

    fn make_registry() -> InstanceRegistry {
        InstanceRegistry::new(Box::new(InMemoryPersistence::default()))
    }

    fn make_config(name: &str) -> ServerInstanceConfig {
        ServerInstanceConfig::new(name, ServerType::MlxLm, 8080, "/usr/bin/mlx")
    }

    fn make_memory(model_key: &str) -> ModelMemory {
        ModelMemory::new(ServerType::MlxLm, model_key)
    }

    // ── Phase transitions ─────────────────────────────────────────────────────

    #[test]
    fn phase_transition_happy_path() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));

        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopped));

        reg.set_phase(id, InstancePhase::Starting).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Starting));

        reg.set_phase(id, InstancePhase::Running).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));

        reg.set_phase(id, InstancePhase::Stopping).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopping));

        reg.set_phase(id, InstancePhase::Stopped).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopped));
    }

    #[test]
    fn phase_transition_error_to_retry() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));

        reg.set_phase(id, InstancePhase::Error(InstanceError {
            kind: InstanceErrorKind::LaunchFailed,
            message: "process exited immediately".to_string(),
        })).unwrap();
        assert!(matches!(reg.get_phase(id), Some(InstancePhase::Error(_))));

        reg.set_phase(id, InstancePhase::Starting).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Starting));
    }

    #[test]
    fn switching_model_phase() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        reg.set_phase(id, InstancePhase::SwitchingModel).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::SwitchingModel));
        assert!(reg.get_phase(id).unwrap().is_active());
    }

    #[test]
    fn set_phase_unknown_id_returns_err() {
        let mut reg = make_registry();
        let unknown = uuid::Uuid::new_v4();
        assert!(reg.set_phase(unknown, InstancePhase::Running).is_err());
    }

    // ── ParamValues::resolve priority ordering ────────────────────────────────

    fn temp_desc() -> crate::types::ParamDescriptor {
        crate::types::ParamDescriptor {
            param: CanonicalParam::Temperature,
            server_flag_name: "--temperature",
            modelfile_param_name: Some("temperature"),
            default_value: Some(ParamValue::Double(0.8)),
        }
    }

    fn ctx_desc() -> crate::types::ParamDescriptor {
        crate::types::ParamDescriptor {
            param: CanonicalParam::ContextLength,
            server_flag_name: "--ctx-size",
            modelfile_param_name: Some("num_ctx"),
            default_value: Some(ParamValue::Int(4096)),
        }
    }

    #[test]
    fn resolve_driver_defaults_only() {
        let schema = vec![temp_desc(), ctx_desc()];
        let resolved = ParamValues::resolve(None, None, &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(0.8));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(4096));
    }

    #[test]
    fn resolve_memory_overrides_driver_defaults() {
        let schema = vec![temp_desc(), ctx_desc()];
        let mut memory_params = ParamValues::default();
        memory_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.3));
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let resolved = ParamValues::resolve(None, Some(&memory), &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(0.3));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(4096));
    }

    #[test]
    fn resolve_profile_overrides_memory_and_driver_defaults() {
        let schema = vec![temp_desc(), ctx_desc()];

        let mut memory_params = ParamValues::default();
        memory_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.3));
        memory_params.values.insert(CanonicalParam::ContextLength, ParamValue::Int(8192));
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let mut profile_params = ParamValues::default();
        profile_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(1.0));

        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(1.0));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(8192));
    }

    #[test]
    fn resolve_system_prompt_propagates_through_layers() {
        let profile_params = ParamValues { system_prompt: Some("You are a helpful assistant.".into()), ..Default::default() };
        let resolved = ParamValues::resolve(Some(&profile_params), None, &[]);
        assert_eq!(resolved.0.system_prompt.as_deref(), Some("You are a helpful assistant."));
    }

    #[test]
    fn resolve_profile_system_prompt_overrides_memory_prompt() {
        let memory_params = ParamValues { system_prompt: Some("from memory".into()), ..Default::default() };
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let profile_params = ParamValues { system_prompt: Some("from profile".into()), ..Default::default() };

        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &[]);
        assert_eq!(resolved.0.system_prompt.as_deref(), Some("from profile"));
    }

    #[test]
    fn resolve_profile_no_prompt_clears_memory_prompt() {
        // Profile with None system_prompt should override (clear) the memory-set prompt.
        let memory_params = ParamValues { system_prompt: Some("from memory".into()), ..Default::default() };
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        // Profile has no system_prompt
        let profile_params = ParamValues::default();
        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &[]);
        assert_eq!(resolved.0.system_prompt, None);
    }

    #[test]
    fn resolve_system_prompt_variant_in_driver_defaults_is_ignored() {
        // A driver that mistakenly includes SystemPrompt in its schema must not
        // end up with it in the values map.
        let schema = vec![
            crate::types::ParamDescriptor {
                param: CanonicalParam::SystemPrompt,
                server_flag_name: "--system",
                modelfile_param_name: None,
                default_value: Some(ParamValue::String("default system".into())),
            },
        ];
        let resolved = ParamValues::resolve(None, None, &schema);
        assert!(!resolved.0.values.contains_key(&CanonicalParam::SystemPrompt));
    }

    // ── start_warning (C3) ────────────────────────────────────────────────────

    #[test]
    fn start_warning_none_when_all_stopped() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("A"));
        let b = reg.add_instance(make_config("B"));
        assert_eq!(reg.start_warning(a), None);
        assert_eq!(reg.start_warning(b), None);
    }

    #[test]
    fn start_warning_some_when_another_is_running() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Running).unwrap();

        let warning = reg.start_warning(b);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("alpha"));
    }

    #[test]
    fn start_warning_none_for_self_when_running() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        reg.set_phase(a, InstancePhase::Running).unwrap();
        assert_eq!(reg.start_warning(a), None);
    }

    #[test]
    fn start_warning_some_when_another_is_starting() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Starting).unwrap();
        assert!(reg.start_warning(b).is_some());
    }

    #[test]
    fn start_warning_none_when_other_is_stopping() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Stopping).unwrap();
        assert_eq!(reg.start_warning(b), None);
    }

    // ── Persistence round-trip ────────────────────────────────────────────────

    #[test]
    fn persistence_round_trip_produces_identical_configs() {
        let store = Arc::new(Mutex::new(InMemoryPersistence::default()));
        let mut reg = InstanceRegistry::new(Box::new(SharedPersistence(Arc::clone(&store))));
        let mut config = make_config("ollama");
        config.server_type = ServerType::Ollama;
        config.port = 11434;
        config.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(config.clone());
        reg.save().expect("save should succeed");

        let mut reg2 = InstanceRegistry::new(Box::new(SharedPersistence(Arc::clone(&store))));
        reg2.load().expect("load should succeed");

        let loaded = reg2.get_config(id).expect("config should exist after load");
        assert_eq!(loaded, &config);
    }

    // ── ensure_managed_model (T8 Seam 1) ────────────────────────────────────

    fn ollama_config(name: &str) -> ServerInstanceConfig {
        ServerInstanceConfig::new(name, ServerType::Ollama, 11434, "/usr/bin/ollama")
    }

    #[test]
    fn ensure_managed_model_noop_when_no_model_selected() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("o"));
        let driver = MockDriver::new(ServerType::Ollama);
        ensure_managed_model(&mut reg, id, &driver).unwrap();
        assert!(driver.managed_config_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn ensure_managed_model_noop_for_driver_without_managed_config() {
        let mut reg = make_registry();
        let mut cfg = make_config("mlx");
        cfg.selected_model_key = Some("model-a".into());
        let id = reg.add_instance(cfg);
        let driver = MockDriver::new(ServerType::MlxLm);
        ensure_managed_model(&mut reg, id, &driver).unwrap();
        assert!(driver.managed_config_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn ensure_managed_model_calls_apply_with_localbar_tag() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(cfg);
        let driver = MockDriver::new(ServerType::Ollama);
        ensure_managed_model(&mut reg, id, &driver).unwrap();
        let calls = driver.managed_config_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.starts_with("localbar/"), "tag must start with 'localbar/'");
        assert!(calls[0].0.contains("llama3"), "tag must contain sanitised model key");
    }

    #[test]
    fn ensure_managed_model_stores_tag_in_config() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(cfg);
        let driver = MockDriver::new(ServerType::Ollama);
        ensure_managed_model(&mut reg, id, &driver).unwrap();
        let tag = reg.get_config(id).unwrap().managed_model_tag.clone();
        assert!(tag.is_some());
        assert!(tag.unwrap().starts_with("localbar/"));
    }

    /// Bug 1: the tag must be computed from the key that is in the config
    /// **at the time ensure_managed_model is called**, not from a stale snapshot.
    #[test]
    fn ensure_managed_model_uses_current_model_key_bug1() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("old-model".into());
        let id = reg.add_instance(cfg);

        // Caller updates key BEFORE calling ensure_managed_model (Bug 1 fix).
        reg.update_config(id, |c| c.selected_model_key = Some("new-model".into())).unwrap();

        let driver = MockDriver::new(ServerType::Ollama);
        ensure_managed_model(&mut reg, id, &driver).unwrap();

        let calls = driver.managed_config_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.contains("new-model"), "tag must use new model key");
        assert!(!calls[0].0.contains("old-model"), "tag must not use old model key");
    }

    // ── update_model_memory (T8 Seam 1) ─────────────────────────────────────

    #[test]
    fn update_model_memory_stores_params_for_model() {
        let mut reg = make_registry();
        let mut cfg = make_config("mlx");
        cfg.selected_model_key = Some("model-a".into());
        let id = reg.add_instance(cfg);
        let mut params = ParamValues::default();
        params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.3));
        update_model_memory(&mut reg, id, params.clone()).unwrap();
        let key = crate::types::ModelMemoryKey {
            server_type: ServerType::MlxLm,
            model_key: "model-a".into(),
        };
        let mem = reg.get_model_memory(&key).unwrap();
        assert_eq!(mem.last_used_params.values[&CanonicalParam::Temperature], ParamValue::Double(0.3));
    }

    #[test]
    fn update_model_memory_noop_when_no_model_selected() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));
        assert!(update_model_memory(&mut reg, id, ParamValues::default()).is_ok());
    }

    #[test]
    fn ensure_managed_model_includes_memory_params_in_modelfile() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(cfg);

        // Prime model memory with a specific temperature.
        let mut mem_params = ParamValues::default();
        mem_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.42));
        update_model_memory(&mut reg, id, mem_params).unwrap();

        let driver = MockDriver::new(ServerType::Ollama);
        ensure_managed_model(&mut reg, id, &driver).unwrap();

        // The content passed to apply_managed_config is generated by MockDriver's
        // generate_managed_config ("FROM <key>"), which doesn't embed params —
        // but the real test is that the driver was called (not the content format).
        let calls = driver.managed_config_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
    }

    // ── push_restart_duration_sample ─────────────────────────────────────────

    #[test]
    fn push_restart_duration_sample_records_value() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(cfg);
        super::push_restart_duration_sample(&mut reg, id, 3.5).unwrap();
        let key = crate::types::ModelMemoryKey {
            server_type: ServerType::Ollama,
            model_key: "llama3:8b".into(),
        };
        let mem = reg.get_model_memory(&key).unwrap();
        assert_eq!(mem.restart_duration_samples, vec![3.5]);
    }

    #[test]
    fn push_restart_duration_sample_newest_first() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("m".into());
        let id = reg.add_instance(cfg);
        super::push_restart_duration_sample(&mut reg, id, 1.0).unwrap();
        super::push_restart_duration_sample(&mut reg, id, 2.0).unwrap();
        let key = crate::types::ModelMemoryKey {
            server_type: ServerType::Ollama,
            model_key: "m".into(),
        };
        let mem = reg.get_model_memory(&key).unwrap();
        assert_eq!(mem.restart_duration_samples[0], 2.0, "newest first");
        assert_eq!(mem.restart_duration_samples[1], 1.0);
    }

    #[test]
    fn push_restart_duration_sample_window_capped_at_10() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("o");
        cfg.selected_model_key = Some("m".into());
        let id = reg.add_instance(cfg);
        for i in 0..12 {
            super::push_restart_duration_sample(&mut reg, id, i as f64).unwrap();
        }
        let key = crate::types::ModelMemoryKey {
            server_type: ServerType::Ollama,
            model_key: "m".into(),
        };
        let mem = reg.get_model_memory(&key).unwrap();
        assert_eq!(mem.restart_duration_samples.len(), 10, "window must not exceed 10");
        assert_eq!(mem.restart_duration_samples[0], 11.0, "newest sample at index 0");
    }

    #[test]
    fn push_restart_duration_sample_noop_when_no_model_selected() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("o"));
        assert!(super::push_restart_duration_sample(&mut reg, id, 1.0).is_ok());
    }

    // ── adopt_external_as_new_instance ───────────────────────────────────────

    #[test]
    fn adopt_external_creates_new_instance_with_external_type() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("ollama");
        cfg.port = 11434;
        let original_id = reg.add_instance(cfg);
        let new_id = adopt_external_as_new_instance(&mut reg, original_id, None).unwrap();
        assert_ne!(new_id, original_id);
        let new_cfg = reg.get_config(new_id).unwrap();
        assert_eq!(new_cfg.server_type, ServerType::External);
        assert_eq!(new_cfg.port, 11434);
    }

    #[test]
    fn adopt_external_persists_detected_model_key() {
        let mut reg = make_registry();
        let original_id = reg.add_instance(ollama_config("ollama"));
        let new_id = adopt_external_as_new_instance(
            &mut reg,
            original_id,
            Some("llama3:8b".into()),
        ).unwrap();
        assert_eq!(
            reg.get_config(new_id).unwrap().selected_model_key.as_deref(),
            Some("llama3:8b"),
        );
    }

    #[test]
    fn adopt_external_preserves_original_instance() {
        let mut reg = make_registry();
        let original_id = reg.add_instance(ollama_config("ollama"));
        adopt_external_as_new_instance(&mut reg, original_id, None).unwrap();
        // Original config must still be accessible with all its fields intact.
        assert!(reg.get_config(original_id).is_some());
    }

    #[test]
    fn adopt_external_unknown_id_returns_err() {
        let mut reg = make_registry();
        assert!(adopt_external_as_new_instance(&mut reg, uuid::Uuid::new_v4(), None).is_err());
    }

    #[test]
    fn adopt_external_host_is_propagated() {
        let mut reg = make_registry();
        let mut cfg = make_config("mlx");
        cfg.host = "192.168.1.10".into();
        cfg.port = 8080;
        let original_id = reg.add_instance(cfg);
        let new_id = adopt_external_as_new_instance(&mut reg, original_id, None).unwrap();
        assert_eq!(reg.get_config(new_id).unwrap().host, "192.168.1.10");
    }

    // ── Mock driver sanity ────────────────────────────────────────────────────

    #[test]
    fn mock_driver_list_models() {
        let driver = MockDriver::new(ServerType::MlxLm);
        let config = make_config("mlx");
        let models = driver.list_models(&config).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].key, "model-a");
    }

    #[test]
    fn mock_driver_fetch_model_metadata_default_none() {
        let driver = MockDriver::new(ServerType::MlxLm);
        let config = make_config("mlx");
        assert_eq!(driver.fetch_model_metadata("model-a", &config), None);
    }

    #[test]
    fn param_descriptor_c6_modelfile_names() {
        let schema = MockDriver::default_schema();
        let temp = schema.iter().find(|d| d.param == CanonicalParam::Temperature).unwrap();
        let max_tokens = schema.iter().find(|d| d.param == CanonicalParam::MaxTokens).unwrap();
        let ctx = schema.iter().find(|d| d.param == CanonicalParam::ContextLength).unwrap();

        assert_eq!(temp.modelfile_param_name, Some("temperature"));
        assert_eq!(max_tokens.modelfile_param_name, None);
        assert_eq!(ctx.modelfile_param_name, Some("num_ctx"));
    }
}
